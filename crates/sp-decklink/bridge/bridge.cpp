// SoftPresenter's bridge to the DeckLink API: a few plain C functions around Blackmagic
// Design's C++ interfaces, for `src/ffi.rs` to call.
//
// Copyright (C) 2026 Esrael Nate and the SoftPresenter contributors.
// SPDX-License-Identifier: GPL-3.0-or-later WITH AdditionRef-SoftPresenter-additional-permission
//
// It is compiled with the DeckLink SDK's headers in third_party/decklink-sdk (copied unchanged,
// under the notice at the top of each), and it is SoftPresenter's own code: nothing from the
// SDK's sample programs is in it. The driver itself (Desktop Video) is installed by the person
// using SoftPresenter and opened while it runs; nothing is linked with it.
//
// How it is kept safe to call from Rust:
// - Only plain C types cross: numbers, pointers to this file's own structures, and buffers the
//   caller provides with their length. No C++ exception, and no Rust code, ever runs inside a
//   call from the driver: the callbacks the driver makes (a frame shown, a device plugged in, a
//   profile changed) only note what happened under a lock and wake whoever waits; Rust asks.
// - Every object this file hands out is freed by exactly one function named for it.
// - The driver is looked for at a path the caller gives (so a test can give one where there
//   is nothing), and not finding it is not remembered: it is looked for again next time.
//
// The names of the driver's entry points (`CreateDeckLinkIteratorInstance_0004` and so on) are
// those SDK 16.0's DeckLinkAPIDispatch.cpp looks up; that file itself is not used, because it
// opens the driver once, from one fixed place, and never again.

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <deque>
#include <mutex>
#include <new>

#include "DeckLinkAPI.h"

#if defined(__APPLE__)
#include <CoreFoundation/CoreFoundation.h>
typedef CFStringRef DeckLinkText;
#else
#include <dlfcn.h>
typedef const char* DeckLinkText;
#endif

extern "C" {

// What the caller is told about a device. Text is UTF-8, cut short to fit, and always ends
// with a zero byte.
struct SpDlDevice {
    char display_name[256];
    char model_name[256];
    int64_t persistent_id;     // -1 when the device has none
    int64_t sub_device;        // -1 when not said
    int64_t sub_devices;       // -1 when not said
    int64_t profile;           // BMDProfileID, 0 when the device has one way only
    int64_t duplex;            // BMDDuplexMode, 0 when not said
    int64_t connections;       // BMDVideoConnection bits it can play out on
    int32_t can_play;          // 1 when it can play video out
    int32_t internal_keying;   // 1 when its keyer can lay pictures over its input
    int32_t external_keying;   // 1 when it can send fill and key on two connectors
    int32_t modes;             // how many of the modes that follow are this device's
    // The format the device plays out in by default, as Desktop Video Setup sets it: the
    // BMDDisplayMode that bmdDeckLinkConfigDefaultVideoOutputMode holds, as the driver gives
    // it, or -1 when the driver doesn't say. With it, bmdDeckLinkConfigDefaultVideoOutputModeFlags
    // (BMDVideoOutputFlags: whether that format is meant as dual-stream 3D), 0 when not said.
    int64_t default_mode;
    int64_t default_mode_flags;
};

// One way a device can play out.
struct SpDlMode {
    uint32_t mode;             // BMDDisplayMode
    uint32_t field_dominance;  // BMDFieldDominance
    int32_t width;
    int32_t height;
    int64_t frame_duration;    // a frame lasts frame_duration / time_scale seconds
    int64_t time_scale;
    int32_t bgra;              // 1 when 8-bit BGRA can be played in it
    int32_t yuv;               // 1 when 8-bit YUV 4:2:2 can
    int32_t keying;            // 1 when 8-bit BGRA can be keyed in it
    char name[64];             // the driver's own name for it
};

// A frame the card has finished with, and how: BMDOutputFrameCompletionResult.
struct SpDlCompletion {
    void* frame;
    uint32_t result;
};

// Flags `sp_dl_out_wait` gives beside the completions.
enum {
    SP_DL_STOPPED = 1,           // the card says scheduled playback has stopped
    SP_DL_PROFILE_CHANGING = 2,  // the card is about to change profile, and streams stop
    SP_DL_PROFILE_CHANGED = 4,   // a new profile is active
};

}  // extern "C"

namespace {

void put_text(char* out, size_t len, const char* text) {
    if (out != nullptr && len > 0) {
        snprintf(out, len, "%s", text);
    }
}

// Copies a string the driver gave into `out`, and frees the driver's.
void take_text(DeckLinkText text, char* out, size_t len) {
    out[0] = 0;
    if (text == nullptr) {
        return;
    }
#if defined(__APPLE__)
    if (!CFStringGetCString(text, out, (CFIndex)len, kCFStringEncodingUTF8)) {
        out[0] = 0;
    }
    CFRelease(text);
#else
    snprintf(out, len, "%s", text);
    free((void*)text);
#endif
}

bool same_iid(const REFIID& a, const REFIID& b) {
    return memcmp(&a, &b, sizeof(REFIID)) == 0;
}

bool is_unknown(const REFIID& iid) {
#if defined(__APPLE__)
    return same_iid(iid, CFUUIDGetUUIDBytes(IUnknownUUID));
#else
    REFIID unknown = IID_IUnknown;
    return same_iid(iid, unknown);
#endif
}

// The driver's entry points, once found.
typedef IDeckLinkIterator* (*CreateIterator)(void);
typedef IDeckLinkAPIInformation* (*CreateInformation)(void);
typedef IDeckLinkDiscovery* (*CreateDiscovery)(void);

std::mutex g_loading;
CreateIterator g_iterator = nullptr;
CreateInformation g_information = nullptr;
CreateDiscovery g_discovery = nullptr;

bool loaded() {
    std::lock_guard<std::mutex> hold(g_loading);
    return g_iterator != nullptr;
}

IDeckLinkIterator* new_iterator() {
    CreateIterator create;
    {
        std::lock_guard<std::mutex> hold(g_loading);
        create = g_iterator;
    }
    return create != nullptr ? create() : nullptr;
}

template <typename T>
void release(T*& object) {
    if (object != nullptr) {
        object->Release();
        object = nullptr;
    }
}

// Sets `out` to the interface `iid` of `object`, or to null. True when it is there.
template <typename T>
bool query(IUnknown* object, const REFIID& iid, T** out) {
    *out = nullptr;
    if (object == nullptr) {
        return false;
    }
    if (object->QueryInterface(iid, (void**)out) != S_OK) {
        *out = nullptr;
        return false;
    }
    return true;
}

int64_t int_attribute(IDeckLinkProfileAttributes* attributes, BMDDeckLinkAttributeID id, int64_t otherwise) {
    int64_t value = 0;
    if (attributes != nullptr && attributes->GetInt(id, &value) == S_OK) {
        return value;
    }
    return otherwise;
}

bool flag_attribute(IDeckLinkProfileAttributes* attributes, BMDDeckLinkAttributeID id) {
    bool value = false;
    return attributes != nullptr && attributes->GetFlag(id, &value) == S_OK && value;
}

// Finds the device with this persistent id (when `id` is not negative) or else this display
// name. The caller releases what it is given.
IDeckLink* find_device(int64_t id, const char* name) {
    IDeckLinkIterator* iterator = new_iterator();
    if (iterator == nullptr) {
        return nullptr;
    }
    IDeckLink* found = nullptr;
    IDeckLink* device = nullptr;
    while (found == nullptr && iterator->Next(&device) == S_OK) {
        IDeckLinkProfileAttributes* attributes = nullptr;
        query(device, IID_IDeckLinkProfileAttributes, &attributes);
        bool same = false;
        if (id >= 0) {
            same = int_attribute(attributes, BMDDeckLinkPersistentID, -1) == id;
        } else {
            DeckLinkText text = nullptr;
            char display[256];
            display[0] = 0;
            if (device->GetDisplayName(&text) == S_OK) {
                take_text(text, display, sizeof display);
            }
            same = name != nullptr && strcmp(display, name) == 0;
        }
        release(attributes);
        if (same) {
            found = device;
        } else {
            device->Release();
        }
        device = nullptr;
    }
    iterator->Release();
    return found;
}

// Told by the driver when a frame has been shown (or dropped), and when playback stops.
class Completions : public IDeckLinkVideoOutputCallback {
public:
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, LPVOID* out) override {
        if (is_unknown(iid) || same_iid(iid, IID_IDeckLinkVideoOutputCallback)) {
            AddRef();
            *out = static_cast<IDeckLinkVideoOutputCallback*>(this);
            return S_OK;
        }
        *out = nullptr;
        return E_NOINTERFACE;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return ++refs_; }
    ULONG STDMETHODCALLTYPE Release() override {
        ULONG left = --refs_;
        if (left == 0) {
            delete this;
        }
        return left;
    }
    HRESULT STDMETHODCALLTYPE ScheduledFrameCompleted(IDeckLinkVideoFrame* frame,
                                                      BMDOutputFrameCompletionResult result) override {
        {
            std::lock_guard<std::mutex> hold(lock_);
            done_.push_back(SpDlCompletion{frame, (uint32_t)result});
        }
        wake_.notify_all();
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE ScheduledPlaybackHasStopped() override {
        note(SP_DL_STOPPED);
        return S_OK;
    }

    void note(uint32_t flag) {
        {
            std::lock_guard<std::mutex> hold(lock_);
            flags_ |= flag;
        }
        wake_.notify_all();
    }

    // Waits up to `timeout_ms` for a completion or a flag, then hands over what there is.
    int32_t take(SpDlCompletion* out, int32_t capacity, int32_t timeout_ms, uint32_t* flags) {
        std::unique_lock<std::mutex> hold(lock_);
        if (done_.empty() && flags_ == 0 && timeout_ms > 0) {
            wake_.wait_for(hold, std::chrono::milliseconds(timeout_ms),
                           [this] { return !done_.empty() || flags_ != 0; });
        }
        int32_t count = 0;
        while (count < capacity && !done_.empty()) {
            out[count++] = done_.front();
            done_.pop_front();
        }
        *flags = flags_;
        flags_ = 0;
        return count;
    }

private:
    std::atomic<ULONG> refs_{1};
    std::mutex lock_;
    std::condition_variable wake_;
    std::deque<SpDlCompletion> done_;
    uint32_t flags_ = 0;
};

// Told by the driver when the device's profile is about to change, and has changed.
class ProfileWatch : public IDeckLinkProfileCallback {
public:
    explicit ProfileWatch(Completions* tell) : tell_(tell) { tell_->AddRef(); }
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, LPVOID* out) override {
        if (is_unknown(iid) || same_iid(iid, IID_IDeckLinkProfileCallback)) {
            AddRef();
            *out = static_cast<IDeckLinkProfileCallback*>(this);
            return S_OK;
        }
        *out = nullptr;
        return E_NOINTERFACE;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return ++refs_; }
    ULONG STDMETHODCALLTYPE Release() override {
        ULONG left = --refs_;
        if (left == 0) {
            delete this;
        }
        return left;
    }
    HRESULT STDMETHODCALLTYPE ProfileChanging(IDeckLinkProfile*, bool) override {
        tell_->note(SP_DL_PROFILE_CHANGING);
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE ProfileActivated(IDeckLinkProfile*) override {
        tell_->note(SP_DL_PROFILE_CHANGED);
        return S_OK;
    }

private:
    ~ProfileWatch() { tell_->Release(); }
    std::atomic<ULONG> refs_{1};
    Completions* tell_;
};

// Told by the driver when a device is plugged in or taken away: counts the changes.
class Arrivals : public IDeckLinkDeviceNotificationCallback {
public:
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, LPVOID* out) override {
        if (is_unknown(iid) || same_iid(iid, IID_IDeckLinkDeviceNotificationCallback)) {
            AddRef();
            *out = static_cast<IDeckLinkDeviceNotificationCallback*>(this);
            return S_OK;
        }
        *out = nullptr;
        return E_NOINTERFACE;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return ++refs_; }
    ULONG STDMETHODCALLTYPE Release() override {
        ULONG left = --refs_;
        if (left == 0) {
            delete this;
        }
        return left;
    }
    HRESULT STDMETHODCALLTYPE DeckLinkDeviceArrived(IDeckLink*) override {
        changed();
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE DeckLinkDeviceRemoved(IDeckLink*) override {
        changed();
        return S_OK;
    }

    uint64_t wait(uint64_t seen, int32_t timeout_ms) {
        std::unique_lock<std::mutex> hold(lock_);
        if (changes_ == seen && timeout_ms > 0) {
            wake_.wait_for(hold, std::chrono::milliseconds(timeout_ms), [&] { return changes_ != seen; });
        }
        return changes_;
    }

private:
    void changed() {
        {
            std::lock_guard<std::mutex> hold(lock_);
            changes_++;
        }
        wake_.notify_all();
    }
    std::atomic<ULONG> refs_{1};
    std::mutex lock_;
    std::condition_variable wake_;
    uint64_t changes_ = 0;
};

}  // namespace

// An output that is open: the device, its interfaces, and what is set on it.
struct SpDlOut {
    IDeckLink* device = nullptr;
    IDeckLinkOutput* output = nullptr;
    IDeckLinkKeyer* keyer = nullptr;
    IDeckLinkConfiguration* config = nullptr;
    IDeckLinkProfileManager* profiles = nullptr;
    Completions* completions = nullptr;
    ProfileWatch* watch = nullptr;
    bool enabled = false;
    bool playing = false;
    bool keying = false;
    bool idle_changed = false;
    int64_t idle_before = 0;
};

// Device notifications that are installed.
struct SpDlDiscovery {
    IDeckLinkDiscovery* discovery = nullptr;
    Arrivals* arrivals = nullptr;
};

extern "C" {

// Opens the driver at `path` (on a Mac its framework, on Linux its library) if it isn't open
// yet. 1 when it is open; 0, with the reason in `err`, when it isn't.
int32_t sp_dl_load(const char* path, char* err, size_t err_len) {
    std::lock_guard<std::mutex> hold(g_loading);
    if (g_iterator != nullptr) {
        return 1;
    }
#if defined(__APPLE__)
    CFStringRef where = CFStringCreateWithCString(kCFAllocatorDefault, path, kCFStringEncodingUTF8);
    if (where == nullptr) {
        put_text(err, err_len, "The DeckLink driver's path could not be read.");
        return 0;
    }
    CFURLRef url = CFURLCreateWithFileSystemPath(kCFAllocatorDefault, where, kCFURLPOSIXPathStyle, true);
    CFRelease(where);
    if (url == nullptr) {
        put_text(err, err_len, "The DeckLink driver's path could not be read.");
        return 0;
    }
    // The bundle is kept for as long as the app runs, as the functions found in it are.
    CFBundleRef bundle = CFBundleCreate(kCFAllocatorDefault, url);
    CFRelease(url);
    if (bundle == nullptr) {
        put_text(err, err_len, "Desktop Video (Blackmagic's DeckLink driver) isn't installed on this computer.");
        return 0;
    }
    CreateIterator iterator =
        (CreateIterator)CFBundleGetFunctionPointerForName(bundle, CFSTR("CreateDeckLinkIteratorInstance_0004"));
    CreateInformation information = (CreateInformation)CFBundleGetFunctionPointerForName(
        bundle, CFSTR("CreateDeckLinkAPIInformationInstance_0001"));
    CreateDiscovery discovery =
        (CreateDiscovery)CFBundleGetFunctionPointerForName(bundle, CFSTR("CreateDeckLinkDiscoveryInstance_0003"));
#else
    // Kept open for as long as the app runs, as the functions found in it are.
    void* library = dlopen(path, RTLD_NOW | RTLD_GLOBAL);
    if (library == nullptr) {
        put_text(err, err_len, "Desktop Video (Blackmagic's DeckLink driver) isn't installed on this computer.");
        return 0;
    }
    CreateIterator iterator = (CreateIterator)dlsym(library, "CreateDeckLinkIteratorInstance_0004");
    CreateInformation information = (CreateInformation)dlsym(library, "CreateDeckLinkAPIInformationInstance_0001");
    CreateDiscovery discovery = (CreateDiscovery)dlsym(library, "CreateDeckLinkDiscoveryInstance_0003");
#endif
    if (iterator == nullptr) {
        put_text(err, err_len,
                 "The DeckLink driver on this computer is too old or damaged. Install the latest Desktop Video.");
        return 0;
    }
    g_iterator = iterator;
    g_information = information;
    g_discovery = discovery;
    return 1;
}

// The driver's version, as BMDDeckLinkAPIVersion gives it (0x0F030100 for 15.3.1), or 0.
int64_t sp_dl_api_version(void) {
    CreateInformation create;
    {
        std::lock_guard<std::mutex> hold(g_loading);
        create = g_information;
    }
    IDeckLinkAPIInformation* information = create != nullptr ? create() : nullptr;
    if (information == nullptr) {
        return 0;
    }
    int64_t version = 0;
    if (information->GetInt(BMDDeckLinkAPIVersion, &version) != S_OK) {
        version = 0;
    }
    information->Release();
    return version;
}

// Lists every device into `devices` (up to `device_room`), with the format it plays out in by
// default, and the ways each can play out into `modes` (up to `mode_room` in all; each device
// says how many of them are its). Says how many of each it wrote. 1 when the list could be
// made, 0 when the driver isn't open. Only reads the devices' settings; never changes one.
int32_t sp_dl_list(SpDlDevice* devices, int32_t device_room, SpDlMode* modes, int32_t mode_room,
                   int32_t* device_count, int32_t* mode_count) {
    *device_count = 0;
    *mode_count = 0;
    if (!loaded()) {
        return 0;
    }
    IDeckLinkIterator* iterator = new_iterator();
    if (iterator == nullptr) {
        return 0;
    }
    IDeckLink* device = nullptr;
    while (*device_count < device_room && iterator->Next(&device) == S_OK) {
        SpDlDevice* about = &devices[*device_count];
        memset(about, 0, sizeof *about);
        DeckLinkText text = nullptr;
        if (device->GetDisplayName(&text) == S_OK) {
            take_text(text, about->display_name, sizeof about->display_name);
        }
        text = nullptr;
        if (device->GetModelName(&text) == S_OK) {
            take_text(text, about->model_name, sizeof about->model_name);
        }
        IDeckLinkProfileAttributes* attributes = nullptr;
        query(device, IID_IDeckLinkProfileAttributes, &attributes);
        about->persistent_id = int_attribute(attributes, BMDDeckLinkPersistentID, -1);
        about->sub_device = int_attribute(attributes, BMDDeckLinkSubDeviceIndex, -1);
        about->sub_devices = int_attribute(attributes, BMDDeckLinkNumberOfSubDevices, -1);
        about->profile = int_attribute(attributes, BMDDeckLinkProfileID, 0);
        about->duplex = int_attribute(attributes, BMDDeckLinkDuplex, 0);
        about->connections = int_attribute(attributes, BMDDeckLinkVideoOutputConnections, 0);
        about->internal_keying = flag_attribute(attributes, BMDDeckLinkSupportsInternalKeying) ? 1 : 0;
        about->external_keying = flag_attribute(attributes, BMDDeckLinkSupportsExternalKeying) ? 1 : 0;
        int64_t io = int_attribute(attributes, BMDDeckLinkVideoIOSupport, 0);
        release(attributes);
        about->default_mode = -1;
        IDeckLinkConfiguration* config = nullptr;
        if (query(device, IID_IDeckLinkConfiguration, &config)) {
            int64_t value = 0;
            if (config->GetInt(bmdDeckLinkConfigDefaultVideoOutputMode, &value) == S_OK) {
                about->default_mode = value;
            }
            value = 0;
            if (config->GetInt(bmdDeckLinkConfigDefaultVideoOutputModeFlags, &value) == S_OK) {
                about->default_mode_flags = value;
            }
            release(config);
        }

        IDeckLinkOutput* output = nullptr;
        // A device the driver gives no output for (an input only, or an older driver that
        // doesn't know SDK 16's output) can't play.
        if ((io & bmdDeviceSupportsPlayback) != 0 && query(device, IID_IDeckLinkOutput, &output)) {
            about->can_play = 1;
            IDeckLinkDisplayModeIterator* each = nullptr;
            if (output->GetDisplayModeIterator(&each) == S_OK && each != nullptr) {
                IDeckLinkDisplayMode* mode = nullptr;
                while (*mode_count < mode_room && each->Next(&mode) == S_OK) {
                    SpDlMode* way = &modes[*mode_count];
                    memset(way, 0, sizeof *way);
                    way->mode = mode->GetDisplayMode();
                    way->field_dominance = mode->GetFieldDominance();
                    way->width = (int32_t)mode->GetWidth();
                    way->height = (int32_t)mode->GetHeight();
                    BMDTimeValue duration = 0;
                    BMDTimeScale scale = 0;
                    if (mode->GetFrameRate(&duration, &scale) == S_OK) {
                        way->frame_duration = duration;
                        way->time_scale = scale;
                    }
                    text = nullptr;
                    if (mode->GetName(&text) == S_OK) {
                        take_text(text, way->name, sizeof way->name);
                    }
                    bool supported = false;
                    BMDDisplayMode actual = bmdModeUnknown;
                    way->bgra = output->DoesSupportVideoMode(bmdVideoConnectionUnspecified, way->mode,
                                                             bmdFormat8BitBGRA, bmdNoVideoOutputConversion,
                                                             bmdSupportedVideoModeDefault, &actual, &supported) ==
                                        S_OK &&
                                    supported
                                    ? 1
                                    : 0;
                    supported = false;
                    way->yuv = output->DoesSupportVideoMode(bmdVideoConnectionUnspecified, way->mode,
                                                            bmdFormat8BitYUV, bmdNoVideoOutputConversion,
                                                            bmdSupportedVideoModeDefault, &actual, &supported) ==
                                       S_OK &&
                                   supported
                                   ? 1
                                   : 0;
                    supported = false;
                    way->keying = output->DoesSupportVideoMode(bmdVideoConnectionUnspecified, way->mode,
                                                               bmdFormat8BitBGRA, bmdNoVideoOutputConversion,
                                                               bmdSupportedVideoModeKeying, &actual, &supported) ==
                                          S_OK &&
                                      supported
                                      ? 1
                                      : 0;
                    mode->Release();
                    mode = nullptr;
                    about->modes++;
                    (*mode_count)++;
                }
                each->Release();
            }
            output->Release();
        }
        device->Release();
        device = nullptr;
        (*device_count)++;
    }
    iterator->Release();
    return 1;
}

// Starts listening for devices plugged in and taken away. Null when that can't be done.
SpDlDiscovery* sp_dl_discovery_start(void) {
    CreateDiscovery create;
    {
        std::lock_guard<std::mutex> hold(g_loading);
        create = g_discovery;
    }
    IDeckLinkDiscovery* discovery = create != nullptr ? create() : nullptr;
    if (discovery == nullptr) {
        return nullptr;
    }
    Arrivals* arrivals = new (std::nothrow) Arrivals();
    if (arrivals == nullptr) {
        discovery->Release();
        return nullptr;
    }
    if (discovery->InstallDeviceNotifications(arrivals) != S_OK) {
        arrivals->Release();
        discovery->Release();
        return nullptr;
    }
    SpDlDiscovery* handle = new (std::nothrow) SpDlDiscovery();
    if (handle == nullptr) {
        discovery->UninstallDeviceNotifications();
        arrivals->Release();
        discovery->Release();
        return nullptr;
    }
    handle->discovery = discovery;
    handle->arrivals = arrivals;
    return handle;
}

// Waits up to `timeout_ms` for the count of changes to differ from `seen`, and gives it.
uint64_t sp_dl_discovery_wait(SpDlDiscovery* handle, uint64_t seen, int32_t timeout_ms) {
    return handle->arrivals->wait(seen, timeout_ms);
}

void sp_dl_discovery_stop(SpDlDiscovery* handle) {
    if (handle == nullptr) {
        return;
    }
    handle->discovery->UninstallDeviceNotifications();
    release(handle->discovery);
    release(handle->arrivals);
    delete handle;
}

// Opens the output of the device with this persistent id (or, when `id` is negative, this
// display name). Null, with the driver's result in `result` (or 1 for "no such device"), when
// it can't be opened.
SpDlOut* sp_dl_out_open(int64_t id, const char* name, int32_t* result) {
    *result = 0;
    if (!loaded()) {
        *result = (int32_t)E_FAIL;
        return nullptr;
    }
    IDeckLink* device = find_device(id, name);
    if (device == nullptr) {
        *result = 1;
        return nullptr;
    }
    SpDlOut* out = new (std::nothrow) SpDlOut();
    if (out == nullptr) {
        device->Release();
        *result = (int32_t)E_OUTOFMEMORY;
        return nullptr;
    }
    out->device = device;
    if (!query(device, IID_IDeckLinkOutput, &out->output)) {
        *result = (int32_t)E_NOINTERFACE;
        release(out->device);
        delete out;
        return nullptr;
    }
    // A device without a keyer, configuration or profiles simply goes without them.
    query(device, IID_IDeckLinkKeyer, &out->keyer);
    query(device, IID_IDeckLinkConfiguration, &out->config);
    query(device, IID_IDeckLinkProfileManager, &out->profiles);
    out->completions = new (std::nothrow) Completions();
    if (out->completions == nullptr || out->output->SetScheduledFrameCompletionCallback(out->completions) != S_OK) {
        *result = (int32_t)E_FAIL;
        release(out->completions);
        release(out->profiles);
        release(out->config);
        release(out->keyer);
        release(out->output);
        release(out->device);
        delete out;
        return nullptr;
    }
    if (out->profiles != nullptr) {
        out->watch = new (std::nothrow) ProfileWatch(out->completions);
        if (out->watch != nullptr && out->profiles->SetCallback(out->watch) != S_OK) {
            release(out->watch);
        }
    }
    return out;
}

// Turns video output on in this mode. While it is on, the card holds its last frame whenever
// it has none newer (rather than going black), where it can; that is undone by
// `sp_dl_out_close`. Gives the driver's result.
int32_t sp_dl_out_enable(SpDlOut* out, uint32_t mode) {
    HRESULT result = out->output->EnableVideoOutput((BMDDisplayMode)mode, bmdVideoOutputFlagDefault);
    if (result != S_OK) {
        return (int32_t)result;
    }
    out->enabled = true;
    IDeckLinkProfileAttributes* attributes = nullptr;
    query(out->device, IID_IDeckLinkProfileAttributes, &attributes);
    bool idle = flag_attribute(attributes, BMDDeckLinkSupportsIdleOutput);
    release(attributes);
    if (idle && out->config != nullptr) {
        int64_t before = 0;
        if (out->config->GetInt(bmdDeckLinkConfigVideoOutputIdleOperation, &before) == S_OK &&
            out->config->SetInt(bmdDeckLinkConfigVideoOutputIdleOperation, bmdIdleVideoOutputLastFrame) == S_OK) {
            out->idle_changed = true;
            out->idle_before = before;
        }
    }
    return (int32_t)S_OK;
}

// The bytes in a row of a frame of this pixel format and width, or 0.
int32_t sp_dl_out_row_bytes(SpDlOut* out, uint32_t pixel_format, int32_t width) {
    int32_t row = 0;
    if (out->output->RowBytesForPixelFormat((BMDPixelFormat)pixel_format, width, &row) != S_OK) {
        return 0;
    }
    return row;
}

// Turns the keyer on, laying the pictures' alpha over the input (`external` 0) or sending
// fill and key on two connectors (`external` 1), at full strength; or off (`on` 0). Gives
// the driver's result, or 1 when the device has no keyer.
int32_t sp_dl_out_keyer(SpDlOut* out, int32_t on, int32_t external) {
    if (out->keyer == nullptr) {
        return 1;
    }
    if (on == 0) {
        if (out->keying) {
            out->keyer->Disable();
            out->keying = false;
        }
        return (int32_t)S_OK;
    }
    HRESULT result = out->keyer->Enable(external != 0);
    if (result != S_OK) {
        return (int32_t)result;
    }
    out->keying = true;
    return (int32_t)out->keyer->SetLevel(255);
}

// A new frame of the card's own memory. Null when it can't be made.
void* sp_dl_frame_new(SpDlOut* out, int32_t width, int32_t height, int32_t row_bytes, uint32_t pixel_format) {
    IDeckLinkMutableVideoFrame* frame = nullptr;
    if (out->output->CreateVideoFrame(width, height, row_bytes, (BMDPixelFormat)pixel_format, bmdFrameFlagDefault,
                                      &frame) != S_OK) {
        return nullptr;
    }
    return frame;
}

// Copies `len` bytes into a frame (all of its rows, one after another). Gives the driver's
// result, or 1 when the frame is smaller than that.
int32_t sp_dl_frame_write(void* frame, const uint8_t* bytes, size_t len) {
    IDeckLinkMutableVideoFrame* video = (IDeckLinkMutableVideoFrame*)frame;
    if ((size_t)video->GetRowBytes() * (size_t)video->GetHeight() < len) {
        return 1;
    }
    IDeckLinkVideoBuffer* buffer = nullptr;
    if (!query(video, IID_IDeckLinkVideoBuffer, &buffer)) {
        return (int32_t)E_NOINTERFACE;
    }
    HRESULT result = buffer->StartAccess(bmdBufferAccessWrite);
    if (result == S_OK) {
        void* into = nullptr;
        result = buffer->GetBytes(&into);
        if (result == S_OK && into != nullptr) {
            memcpy(into, bytes, len);
        } else if (result == S_OK) {
            result = E_FAIL;
        }
        buffer->EndAccess(bmdBufferAccessWrite);
    }
    buffer->Release();
    return (int32_t)result;
}

void sp_dl_frame_release(void* frame) {
    if (frame != nullptr) {
        ((IDeckLinkMutableVideoFrame*)frame)->Release();
    }
}

// Schedules a frame to be shown at `time` for `duration` (in units of `scale` a second).
int32_t sp_dl_out_schedule(SpDlOut* out, void* frame, int64_t time, int64_t duration, int64_t scale) {
    return (int32_t)out->output->ScheduleVideoFrame((IDeckLinkVideoFrame*)frame, time, duration, scale);
}

// Starts playing the frames scheduled, from `time`.
int32_t sp_dl_out_start(SpDlOut* out, int64_t time, int64_t scale) {
    HRESULT result = out->output->StartScheduledPlayback(time, scale, 1.0);
    if (result == S_OK) {
        out->playing = true;
    }
    return (int32_t)result;
}

// Where playback is now, in units of `scale` a second. Gives the driver's result.
int32_t sp_dl_out_stream_time(SpDlOut* out, int64_t scale, int64_t* time) {
    double speed = 0.0;
    BMDTimeValue now = 0;
    HRESULT result = out->output->GetScheduledStreamTime(scale, &now, &speed);
    *time = now;
    return (int32_t)result;
}

// Waits up to `timeout_ms` for frames the card has finished with, and gives up to `room` of
// them, with the flags noted since the last call.
int32_t sp_dl_out_wait(SpDlOut* out, SpDlCompletion* done, int32_t room, int32_t timeout_ms, uint32_t* flags) {
    return out->completions->take(done, room, timeout_ms, flags);
}

// Stops playback, turns the keyer and the output off, puts back what the card did with no
// frame to show, and frees everything. Frames still scheduled are given back by the driver
// (as flushed) before this returns; the caller frees its own frames after.
void sp_dl_out_close(SpDlOut* out) {
    if (out == nullptr) {
        return;
    }
    if (out->playing) {
        BMDTimeValue stopped = 0;
        out->output->StopScheduledPlayback(0, &stopped, 0);
        out->playing = false;
    }
    if (out->keying && out->keyer != nullptr) {
        out->keyer->Disable();
        out->keying = false;
    }
    if (out->idle_changed && out->config != nullptr) {
        out->config->SetInt(bmdDeckLinkConfigVideoOutputIdleOperation, out->idle_before);
    }
    if (out->enabled) {
        out->output->DisableVideoOutput();
        out->enabled = false;
    }
    out->output->SetScheduledFrameCompletionCallback(nullptr);
    if (out->profiles != nullptr && out->watch != nullptr) {
        out->profiles->SetCallback(nullptr);
    }
    release(out->watch);
    release(out->completions);
    release(out->profiles);
    release(out->config);
    release(out->keyer);
    release(out->output);
    release(out->device);
    delete out;
}

}  // extern "C"
