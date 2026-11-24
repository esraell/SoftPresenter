//! Stress test of the app's real graphics setup, for finding a hang in a driver: a screens
//! painter and a thumbnails painter on two threads, drawing continuously, and (as the operator
//! window's egui does) a third thread that submits small passes of its own.
//!
//! `two_painters shared <seconds>`: one instance and device for all three threads, as the app
//! has it. `two_painters separate <seconds>`: each thread has an instance and device of its own.
//!
//! A watchdog ends the program with exit code 3 if no thread has drawn for 30 seconds.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use sp_model::{Object, Role, Slide, Theme};
use sp_render::vello::util::RenderContext;
use sp_render::{Fonts, Painter, build_scene, read_pixels, target_texture, wgpu};

#[derive(Clone)]
struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    // Kept so the instance outlives the device.
    _context: Arc<RenderContext>,
}

fn new_gpu() -> Gpu {
    let mut context = RenderContext::new();
    let id = pollster::block_on(context.device(None)).expect("a GPU");
    let handle = &context.devices[id];
    let (device, queue) = (handle.device.clone(), handle.queue.clone());
    Gpu { device, queue, _context: Arc::new(context) }
}

fn painter_thread(gpu: Gpu, (w, h): (u32, u32), read_back: bool, count: Arc<AtomicU64>, stop: Arc<AtomicBool>) {
    let mut painter = Painter::new(&gpu.device).expect("painter");
    let mut fonts = Fonts::bundled_only();
    let theme = Theme::builtin();
    let texture = target_texture(&gpu.device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut n = 0u64;
    while !stop.load(Ordering::Relaxed) {
        let text = format!("Amazing grace, how sweet the sound {n}");
        let slide = Slide::new(vec![Object::text(Role::primary(), &text)]);
        let built = build_scene(&mut fonts, Some(&slide), &theme, w, h);
        if let Err(e) = painter.paint(&gpu.device, &gpu.queue, &built.scene, &view, w, h) {
            eprintln!("paint: {e:?}");
        }
        if read_back && n.is_multiple_of(4) {
            read_pixels(&gpu.device, &gpu.queue, &texture).expect("read back");
        }
        if n.is_multiple_of(50) {
            while !painter.rest(&gpu.device, &gpu.queue) {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        n += 1;
        count.fetch_add(1, Ordering::Relaxed);
    }
}

fn egui_like_thread(gpu: Gpu, count: Arc<AtomicU64>, stop: Arc<AtomicBool>) {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: 256, height: 256, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut n = 0u64;
    while !stop.load(Ordering::Relaxed) {
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.1, g: 0.1, b: 0.1, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        gpu.queue.write_texture(
            texture.as_image_copy(),
            &vec![(n % 255) as u8; 256 * 4 * 16],
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256 * 4), rows_per_image: None },
            wgpu::Extent3d { width: 256, height: 16, depth_or_array_layers: 1 },
        );
        gpu.queue.submit([encoder.finish()]);
        let _ = gpu.device.poll(wgpu::PollType::Poll);
        std::thread::sleep(Duration::from_millis(4));
        n += 1;
        count.fetch_add(1, Ordering::Relaxed);
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_else(|| "shared".into());
    let seconds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(60);
    let shared = match mode.as_str() {
        "shared" => true,
        "separate" => false,
        other => return eprintln!("unknown mode {other}: shared or separate"),
    };
    let stop = Arc::new(AtomicBool::new(false));
    let counts: Vec<Arc<AtomicU64>> = (0..3).map(|_| Arc::new(AtomicU64::new(0))).collect();
    let first = new_gpu();
    let gpus: Vec<Gpu> = (0..3).map(|i| if shared || i == 0 { first.clone() } else { new_gpu() }).collect();
    let mut gpus = gpus.into_iter();
    let (a, b, c) = (gpus.next().unwrap(), gpus.next().unwrap(), gpus.next().unwrap());
    let started = Instant::now();
    let spawn = |name: &str, run: Box<dyn FnOnce() + Send>| {
        std::thread::Builder::new().name(name.into()).spawn(run).expect("thread")
    };
    let threads = vec![
        {
            let (count, stop) = (counts[0].clone(), stop.clone());
            spawn("screens", Box::new(move || painter_thread(a, (1920, 1080), false, count, stop)))
        },
        {
            let (count, stop) = (counts[1].clone(), stop.clone());
            spawn("thumbs", Box::new(move || painter_thread(b, (192, 108), true, count, stop)))
        },
        {
            let (count, stop) = (counts[2].clone(), stop.clone());
            spawn("egui", Box::new(move || egui_like_thread(c, count, stop)))
        },
    ];
    let mut last = (Instant::now(), 0u64);
    while started.elapsed() < Duration::from_secs(seconds) {
        std::thread::sleep(Duration::from_secs(1));
        let total: u64 = counts.iter().map(|c| c.load(Ordering::Relaxed)).sum();
        if total != last.1 {
            last = (Instant::now(), total);
        } else if last.0.elapsed() > Duration::from_secs(30) {
            eprintln!("HANG after {:?}: pictures {counts:?}", started.elapsed());
            std::process::exit(3);
        }
        if started.elapsed().as_secs().is_multiple_of(60) {
            let c: Vec<u64> = counts.iter().map(|c| c.load(Ordering::Relaxed)).collect();
            eprintln!("{:>4}s screens {} thumbs {} egui {}", started.elapsed().as_secs(), c[0], c[1], c[2]);
        }
    }
    stop.store(true, Ordering::Relaxed);
    for t in threads {
        t.join().expect("thread did not panic");
    }
    drop(first);
    eprintln!(
        "{mode}: no hang in {seconds} s, pictures: screens {} thumbs {} egui {}",
        counts[0].load(Ordering::Relaxed),
        counts[1].load(Ordering::Relaxed),
        counts[2].load(Ordering::Relaxed)
    );
}
