//! Checks, on this Mac's GPU, that a video's pictures handed to the GPU as the decoder's own
//! memory (no copy) are the same pixels as the copies, and that the decoder's memory is not used
//! again while a texture over it is held. It needs the main thread (the system's player does),
//! so it is run by hand, not by `cargo test`:
//!
//!     cargo run --release -p sp-video --example zero_copy [clip.mp4]
//!
//! Without a clip it makes two with FFmpeg: a 1920x1080 one (a moving test picture, so any
//! mix-up of rows or channels shows) and a 320x180 one whose every picture is one colour,
//! brighter by the picture's number, so a picture says which it is. It exits with an error when
//! anything differs.

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("Videos go to the GPU without a copy on a Mac only.");
}

#[cfg(target_os = "macos")]
fn main() {
    mac::run();
}

#[cfg(target_os = "macos")]
mod mac {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use sp_video::{GpuImport, GpuNext, Player};

    pub fn run() {
        let (device, queue) = device();
        let import = GpuImport::new(&device).expect("this GPU can make textures of decoded pictures");
        let given = std::env::args().nth(1).map(PathBuf::from);
        let dir = std::env::temp_dir().join(format!("softpresenter-zero-copy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder for the clips");
        let (clip, marker) = match &given {
            Some(path) => (path.clone(), path.clone()),
            None => (make(&dir, "clip.mp4", "testsrc2=size=1920x1080:rate=30:duration=3"), marker(&dir)),
        };
        same_pixels(&clip, &device, &queue, &import);
        held_textures_keep_their_pictures(&marker, &device, &queue, &import);
        let _ = std::fs::remove_dir_all(&dir);
        println!("ok");
    }

    /// Lets the main thread's run loop turn, which is how the system finishes seeks and plays
    /// a video; the app's event loop does it for the app.
    #[allow(unsafe_code)]
    fn spin(time: Duration) {
        use objc2_core_foundation::{CFRunLoop, kCFRunLoopDefaultMode};
        // SAFETY: reads a static the framework defines.
        let mode = unsafe { kCFRunLoopDefaultMode };
        CFRunLoop::run_in_mode(mode, time.as_secs_f64(), false);
    }

    fn device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("a GPU");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("a device")
    }

    fn ffmpeg(args: &[&str], out: &Path) {
        let made = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-y"])
            .args(args)
            .arg(out)
            .status()
            .expect("FFmpeg runs (pass a clip to do without it)");
        assert!(made.success(), "the clip is made");
    }

    fn make(dir: &Path, name: &str, source: &str) -> PathBuf {
        let path = dir.join(name);
        ffmpeg(&["-f", "lavfi", "-i", source, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-b:v", "8M"], &path);
        path
    }

    /// Two seconds of 30 pictures a second, picture N a flat red of 20 + 3 N.
    fn marker(dir: &Path) -> PathBuf {
        let path = dir.join("marker.mp4");
        let source = "color=c=black:s=320x180:r=30:d=2,format=rgb24,geq=r='20+N*3':g='0':b='0'";
        ffmpeg(&["-f", "lavfi", "-i", source, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "10"], &path);
        path
    }

    fn open(path: &Path) -> Player {
        let mut player = Player::open(path, sp_video::Lend::Rows).expect("the system's player opens the clip");
        assert!(!player.uses_ffmpeg(), "the system's player plays it");
        player.set_volume(0.0);
        player
    }

    /// Takes the next picture by copying, waiting up to three seconds: tightly packed BGRA rows.
    fn copied(player: &mut Player) -> (u32, u32, Vec<u8>) {
        let frames = player.frames();
        let until = Instant::now() + Duration::from_secs(3);
        while Instant::now() < until {
            player.tick();
            let taken = frames.next_with(|lending| {
                let lent = lending.rows().expect("the system's player lends rows");
                assert_eq!(lent.order, sp_video::PixelOrder::Bgra);
                let row = lent.width as usize * 4;
                let rows: Vec<u8> = lent
                    .bytes
                    .chunks(lent.bytes_per_row)
                    .take(lent.height as usize)
                    .flat_map(|r| r[..row].to_vec())
                    .collect();
                (lent.width, lent.height, rows)
            });
            if let Some(taken) = taken {
                return taken;
            }
            spin(Duration::from_millis(5));
        }
        panic!("no picture came");
    }

    /// Takes the next picture as a texture, waiting up to three seconds.
    fn textured(player: &mut Player, device: &wgpu::Device, import: &GpuImport) -> wgpu::Texture {
        let frames = player.frames();
        let until = Instant::now() + Duration::from_secs(3);
        while Instant::now() < until {
            player.tick();
            match frames.next_texture(import, device) {
                GpuNext::Picture(texture) => return texture,
                GpuNext::Nothing => spin(Duration::from_millis(5)),
                GpuNext::Unavailable => panic!("this clip's pictures should go to the GPU as they are"),
            }
        }
        panic!("no picture came");
    }

    /// The same moments of the same clip, taken both ways, are the same pixels.
    fn same_pixels(clip: &Path, device: &wgpu::Device, queue: &wgpu::Queue, import: &GpuImport) {
        for at in [0.0, 0.4, 1.3] {
            let (mut by_copy, mut by_texture) = (open(clip), open(clip));
            // Each is played a moment, so the system has it ready, then paused at the same
            // exact time: both decoders are then asked for the same picture.
            for player in [&mut by_copy, &mut by_texture] {
                player.play();
            }
            let until = Instant::now() + Duration::from_secs(3);
            while Instant::now() < until && (by_copy.position() < 0.2 || by_texture.position() < 0.2) {
                by_copy.tick();
                by_texture.tick();
                spin(Duration::from_millis(10));
            }
            for player in [&mut by_copy, &mut by_texture] {
                player.pause();
                player.seek(at);
            }
            // Pictures taken on the way are let go.
            while by_copy.frames().next_with(|_| ()).is_some() {}
            while !matches!(by_texture.frames().next_texture(import, device), GpuNext::Nothing) {}
            spin(Duration::from_millis(300));
            let (w, h, copy) = copied(&mut by_copy);
            let texture = textured(&mut by_texture, device, import);
            assert_eq!((texture.width(), texture.height()), (w, h), "the same size at {at}");
            let gpu = sp_render::read_pixels(device, queue, &texture).expect("read back");
            assert_eq!(gpu.len(), copy.len());
            let wrong = gpu.iter().zip(&copy).filter(|(a, b)| a != b).count();
            assert_eq!(wrong, 0, "{wrong} bytes of {} differ at {at} s", copy.len());
            println!("same pixels at {at} s ({w}x{h})");
        }
    }

    /// Textures held while the clip plays on keep their own pictures: the decoder's memory is
    /// not reused under them. Each picture is one flat red, brighter picture by picture, so a
    /// reused buffer shows as a held texture changing to a later picture's red.
    fn held_textures_keep_their_pictures(clip: &Path, device: &wgpu::Device, queue: &wgpu::Queue, import: &GpuImport) {
        let mut player = open(clip);
        player.set_looping(true);
        player.play();
        let frames = player.frames();
        let mut held: Vec<(wgpu::Texture, u8)> = Vec::new();
        let middle = |texture: &wgpu::Texture| {
            let pixels = sp_render::read_pixels(device, queue, texture).expect("read back");
            let at = ((texture.height() / 2 * texture.width() + texture.width() / 2) * 4) as usize;
            pixels[at + 2] // BGRA: the red
        };
        let until = Instant::now() + Duration::from_secs(4);
        let mut taken = 0;
        while Instant::now() < until && taken < 60 {
            player.tick();
            match frames.next_texture(import, device) {
                GpuNext::Picture(texture) => {
                    taken += 1;
                    // Keep one in three, read as it arrives; the rest are let go at once, as the
                    // screens let go of theirs, so the decoder has memory to reuse.
                    if taken % 3 == 0 && held.len() < 12 {
                        let red = middle(&texture);
                        held.push((texture, red));
                    }
                }
                GpuNext::Nothing => spin(Duration::from_millis(4)),
                GpuNext::Unavailable => panic!("this clip's pictures should go to the GPU as they are"),
            }
        }
        assert!(held.len() >= 8, "enough pictures came: {} held of {taken}", held.len());
        // Many more pictures have been decoded since; each held texture still reads as it did.
        for (n, (texture, red)) in held.iter().enumerate() {
            let now = middle(texture);
            assert_eq!(now, *red, "held texture {n} changed after {taken} pictures");
        }
        let reds: Vec<u8> = held.iter().map(|(_, red)| *red).collect();
        let mut distinct = reds.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert!(distinct.len() >= 6, "the held pictures are different pictures: {reds:?}");
        println!("{} held textures kept their pictures through {taken} pictures: {reds:?}", held.len());
        drop(held);
    }
}
