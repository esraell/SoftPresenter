// A test helper, never part of the app: makes a virtual display on this Mac for a while, and
// lists the windows of a program with where they are.
//
//   vdisplay run <name> <seconds> [width height]   make a display, remove it after <seconds>
//   vdisplay windows <owner> [pid]                 windows of that program (and process): bounds, layer, on screen
//   vdisplay displays                              the active displays and their frames
//
// It uses CGVirtualDisplay, a private interface of CoreGraphics that open-source virtual display
// tools use (declared below from what those tools publish; nothing is copied from them). It
// exists so the app's handling of a display going and coming back can be tried without a second
// monitor. The display ends when the helper does, on its own timer, or earlier if it is killed.

#import <Foundation/Foundation.h>
#import <CoreGraphics/CoreGraphics.h>

@interface CGVirtualDisplayMode : NSObject
- (instancetype)initWithWidth:(unsigned int)width height:(unsigned int)height refreshRate:(double)rate;
@end

@interface CGVirtualDisplaySettings : NSObject
@property(nonatomic) unsigned int hiDPI;
@property(retain, nonatomic) NSArray<CGVirtualDisplayMode *> *modes;
@end

@interface CGVirtualDisplayDescriptor : NSObject
@property(retain, nonatomic) NSString *name;
@property(nonatomic) unsigned int maxPixelsWide;
@property(nonatomic) unsigned int maxPixelsHigh;
@property(nonatomic) CGSize sizeInMillimeters;
@property(nonatomic) unsigned int productID;
@property(nonatomic) unsigned int vendorID;
@property(nonatomic) unsigned int serialNum;
@property(retain, nonatomic) dispatch_queue_t queue;
@end

@interface CGVirtualDisplay : NSObject
- (instancetype)initWithDescriptor:(CGVirtualDisplayDescriptor *)descriptor;
- (BOOL)applySettings:(CGVirtualDisplaySettings *)settings;
@property(readonly, nonatomic) unsigned int displayID;
@end

static void list_displays(void) {
    CGDirectDisplayID ids[16];
    uint32_t count = 0;
    CGGetActiveDisplayList(16, ids, &count);
    for (uint32_t i = 0; i < count; i++) {
        CGRect frame = CGDisplayBounds(ids[i]);
        printf("display %u main=%d frame=%.0f,%.0f %.0fx%.0f\n", ids[i], CGDisplayIsMain(ids[i]) ? 1 : 0,
               frame.origin.x, frame.origin.y, frame.size.width, frame.size.height);
    }
}

static void list_windows(const char *owner, int pid) {
    NSArray *windows = CFBridgingRelease(CGWindowListCopyWindowInfo(kCGWindowListOptionAll, kCGNullWindowID));
    for (NSDictionary *w in windows) {
        NSString *name = w[(id)kCGWindowOwnerName];
        if (![name isEqualToString:[NSString stringWithUTF8String:owner]]) continue;
        if (pid > 0 && [w[(id)kCGWindowOwnerPID] intValue] != pid) continue;
        CGRect bounds = CGRectZero;
        CGRectMakeWithDictionaryRepresentation((CFDictionaryRef)w[(id)kCGWindowBounds], &bounds);
        NSString *line = [NSString stringWithFormat:@"window %@ \"%@\" layer=%@ onscreen=%@ bounds=%.0f,%.0f %.0fx%.0f\n",
                          w[(id)kCGWindowNumber], w[(id)kCGWindowName] ?: @"", w[(id)kCGWindowLayer],
                          w[(id)kCGWindowIsOnscreen] ?: @0, bounds.origin.x, bounds.origin.y, bounds.size.width,
                          bounds.size.height];
        fputs(line.UTF8String, stdout);
    }
}

int main(int argc, char **argv) {
    @autoreleasepool {
        if (argc >= 2 && strcmp(argv[1], "displays") == 0) {
            list_displays();
            return 0;
        }
        if (argc >= 3 && strcmp(argv[1], "windows") == 0) {
            list_windows(argv[2], argc >= 4 ? atoi(argv[3]) : 0);
            return 0;
        }
        if (argc >= 4 && strcmp(argv[1], "run") == 0) {
            unsigned int width = argc >= 6 ? (unsigned)atoi(argv[4]) : 1920;
            unsigned int height = argc >= 6 ? (unsigned)atoi(argv[5]) : 1080;
            double seconds = atof(argv[3]);
            CGVirtualDisplayDescriptor *descriptor = [[CGVirtualDisplayDescriptor alloc] init];
            descriptor.name = [NSString stringWithUTF8String:argv[2]];
            descriptor.maxPixelsWide = width;
            descriptor.maxPixelsHigh = height;
            descriptor.sizeInMillimeters = CGSizeMake(width * 0.25, height * 0.25);
            descriptor.productID = 0x5350;
            descriptor.vendorID = 0x5350;
            descriptor.serialNum = (unsigned)getpid();
            descriptor.queue = dispatch_get_main_queue();
            CGVirtualDisplay *display = [[CGVirtualDisplay alloc] initWithDescriptor:descriptor];
            if (display == nil) {
                fprintf(stderr, "could not make a virtual display\n");
                return 1;
            }
            CGVirtualDisplaySettings *settings = [[CGVirtualDisplaySettings alloc] init];
            settings.hiDPI = 0;
            settings.modes = @[ [[CGVirtualDisplayMode alloc] initWithWidth:width height:height refreshRate:60] ];
            if (![display applySettings:settings]) {
                fprintf(stderr, "could not apply the display's settings\n");
                return 1;
            }
            printf("display %u made (%ux%u), ends in %.0f s\n", display.displayID, width, height, seconds);
            fflush(stdout);
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(seconds * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
                printf("display removed\n");
                fflush(stdout);
                exit(0);
            });
            [[NSRunLoop mainRunLoop] run];
        }
        fprintf(stderr, "usage: vdisplay run <name> <seconds> [w h] | windows <owner> [pid] | displays\n");
        return 2;
    }
}
