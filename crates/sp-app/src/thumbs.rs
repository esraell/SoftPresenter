//! Small pictures of slides for the home window, drawn by the same renderer as the output.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use sp_model::{Slide, Theme, Uuid};
use sp_render::{Fonts, Layers, Painter, Picture, build_layers, target_texture, wgpu};

use crate::Gpu;

/// Thumbnails are drawn at this size and scaled by the interface to whatever is displayed.
pub const THUMB_W: u32 = 480;
pub const THUMB_H: u32 = 270;
const PREVIEW_W: u32 = 640;
const PREVIEW_H: u32 = 360;
/// New thumbnails drawn per frame, so a long song fills in without stalling the window.
const PER_FRAME: usize = 12;
/// Thumbnails kept at once. Each is about half a megabyte of GPU memory, so this is about
/// 80 MB; scrolling a long song drops the ones not looked at for longest.
const MAX_CACHED: usize = 160;

struct Thumb {
    id: egui::TextureId,
    /// The slide it was drawn from: its own key, or the slide a picture of it in a theme
    /// version was drawn from, so forgetting the slide forgets both.
    of: Uuid,
    overflow: bool,
    /// The frame this thumbnail was last shown in.
    last_used: u64,
    /// The addresses of the web page boxes it shows (its slide's and its layout's), so it can be
    /// drawn again when one of those pages has its first picture.
    pages: Vec<String>,
    /// When drawing it failed: it is left blank, and tried again a while later rather than on
    /// every frame.
    failed: Option<std::time::Instant>,
    // Kept alive for as long as the interface may show it.
    _texture: wgpu::Texture,
}

/// How long a thumbnail that couldn't be drawn waits before it is tried again.
const RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(5);

/// The kinds of picture the window's painter draws.
#[derive(Clone, Copy)]
enum Kind {
    WaitingTile,
    Canvas,
    Preview,
}

pub struct Thumbs {
    painter: Painter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// Pictures other than thumbnails that could not be drawn, by kind (see `note_drawn`): each
    /// said once while it lasts, not at every drawing.
    trouble: [crate::applog::Lasting; 3],
    /// The slides whose thumbnail could not be drawn and has been said so: a thumbnail is
    /// tried again every few seconds, and each slide's is said once, whatever the others do.
    thumbnails_said: std::collections::HashSet<Uuid>,
    /// How many pictures the painter had drawn when it was last looked at, and when to let it
    /// rest if it draws no more.
    painted: u64,
    rest_at: Option<Instant>,
    fonts: Fonts,
    cache: HashMap<Uuid, Thumb>,
    drawn_this_frame: usize,
    frame: u64,
    /// The two pictures the preview alternates between, and which one is in front.
    previews: [Option<(wgpu::Texture, egui::TextureId)>; 2],
    preview_front: usize,
    canvas: Option<(wgpu::Texture, egui::TextureId)>,
    /// A plain dark tile, shown for a media item whose picture is still being read.
    waiting: Option<(wgpu::Texture, egui::TextureId)>,
}

impl Thumbs {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Thumbs, String> {
        Ok(Thumbs {
            painter: Painter::new(device)?,
            device: device.clone(),
            queue: queue.clone(),
            trouble: Default::default(),
            thumbnails_said: std::collections::HashSet::new(),
            painted: 0,
            rest_at: None,
            fonts: Fonts::new(),
            cache: HashMap::new(),
            drawn_this_frame: 0,
            frame: 0,
            previews: [None, None],
            preview_front: 0,
            canvas: None,
            waiting: None,
        })
    }

    /// The bytes of GPU memory the painter keeps ready between pictures.
    pub fn spare_bytes(&self) -> u64 {
        self.painter.spare_bytes()
    }

    /// Call once at the start of each frame.
    pub fn begin_frame(&mut self) {
        self.drawn_this_frame = 0;
        self.frame += 1;
    }

    /// Call whenever the app is about to wait. A second after the last thumbnail, preview or
    /// canvas was drawn, GPU memory that a heavy slide made the painter take is given back
    /// (ordinary slides take none worth giving back). Returns when to call again, if that is
    /// still to come: the settling, or, since giving memory back never waits for the GPU, a
    /// moment later while the GPU still holds some of it.
    pub fn rest_when_settled(&mut self, now: Instant) -> Option<Instant> {
        const SETTLED_AFTER: Duration = Duration::from_secs(1);
        const LOOK_AGAIN_AFTER: Duration = Duration::from_millis(50);
        if self.painter.paints() != self.painted {
            self.painted = self.painter.paints();
            self.rest_at = Some(now + SETTLED_AFTER);
        }
        if self.rest_at.is_some_and(|at| at <= now) {
            let rested = self.painter.rest(&self.device, &self.queue);
            self.rest_at = (!rested).then_some(now + LOOK_AGAIN_AFTER);
        }
        self.rest_at
    }

    /// Notes whether a picture of one kind was drawn, and says that it could not be, once for as
    /// long as the same thing goes wrong with that kind.
    fn note_drawn(&mut self, kind: Kind, drawn: &Result<(), sp_render::PaintError>) {
        let trouble = &mut self.trouble[kind as usize];
        match drawn {
            Ok(()) => drop(trouble.over()),
            Err(error) => {
                let what = match kind {
                    Kind::WaitingTile => "a waiting tile",
                    Kind::Canvas => "the slide being edited",
                    Kind::Preview => "the preview",
                };
                let message = format!("Could not draw {what}: {error}");
                if trouble.is_news(&message) {
                    crate::applog::problem("window", &message);
                }
            }
        }
    }

    /// The thumbnail for a slide and whether its text overflows. `None` means it will be
    /// ready on a later frame.
    pub fn get(
        &mut self,
        gpu: &Gpu,
        egui_renderer: &mut egui_wgpu::Renderer,
        slide: &Slide,
        media: Option<(&Picture, sp_model::ImageFit)>,
        theme: &Theme,
    ) -> Option<(egui::TextureId, bool)> {
        self.get_layers(
            gpu,
            egui_renderer,
            (slide.id, slide.id),
            &Layers {
                media_fit: media.map_or(sp_model::ImageFit::Fill, |(_, fit)| fit),
                media: media.map(|(picture, _)| picture),
                slide: Some(slide),
                overlay: None,
                time: 0.0,
                backdrop: None,
                mask: [0.0; 4],
                announcement: None,
            },
            theme,
        )
    }

    /// The thumbnail of a slide as a theme version draws it (`shown`, kept under its own id), made
    /// from the slide whose id is `of`: forgetting that slide forgets this too. `None` means it
    /// will be ready on a later frame.
    pub fn get_in_version(
        &mut self,
        gpu: &Gpu,
        egui_renderer: &mut egui_wgpu::Renderer,
        shown: &Slide,
        of: Uuid,
        media: Option<(&Picture, sp_model::ImageFit)>,
        theme: &Theme,
    ) -> Option<(egui::TextureId, bool)> {
        self.get_layers(
            gpu,
            egui_renderer,
            (shown.id, of),
            &Layers {
                media_fit: media.map_or(sp_model::ImageFit::Fill, |(_, fit)| fit),
                media: media.map(|(picture, _)| picture),
                slide: Some(shown),
                overlay: None,
                time: 0.0,
                backdrop: None,
                mask: [0.0; 4],
                announcement: None,
            },
            theme,
        )
    }

    /// The thumbnail kept under `key`, when there is one to show, without drawing anything.
    pub fn kept(&mut self, key: Uuid) -> Option<(egui::TextureId, bool)> {
        let thumb = self.cache.get_mut(&key)?;
        if thumb.failed.is_some_and(|at| at.elapsed() >= RETRY_AFTER) {
            return None;
        }
        thumb.last_used = self.frame;
        Some((thumb.id, thumb.overflow))
    }

    /// The thumbnail for a picture in the media bin. `None` means it will be ready on a later
    /// frame.
    pub fn get_media(
        &mut self,
        gpu: &Gpu,
        egui_renderer: &mut egui_wgpu::Renderer,
        id: Uuid,
        picture: &Picture,
        theme: &Theme,
    ) -> Option<egui::TextureId> {
        self.get_layers(
            gpu,
            egui_renderer,
            (id, id),
            &Layers {
                media_fit: sp_model::ImageFit::Fill,
                media: Some(picture),
                slide: None,
                overlay: None,
                time: 0.0,
                backdrop: None,
                mask: [0.0; 4],
                announcement: None,
            },
            theme,
        )
        .map(|(id, _)| id)
    }

    /// A plain dark tile for a media item whose picture is still being read: drawn once, and
    /// shared by every item waiting. It is not kept under the item, so the item's own thumbnail
    /// is drawn as soon as its picture is there.
    pub fn waiting(&mut self, gpu: &Gpu, egui_renderer: &mut egui_wgpu::Renderer) -> Option<egui::TextureId> {
        if let Some((_, id)) = &self.waiting {
            return Some(*id);
        }
        const W: u32 = 16;
        const H: u32 = 9;
        let texture = target_texture(&gpu.device, W, H);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let drawn = self.painter.paint(&gpu.device, &gpu.queue, &sp_render::vello::Scene::new(), &view, W, H);
        self.note_drawn(Kind::WaitingTile, &drawn);
        drawn.ok()?;
        let id = egui_renderer.register_native_texture(&gpu.device, &view, wgpu::FilterMode::Linear);
        self.waiting = Some((texture, id));
        Some(id)
    }

    /// Drops one thumbnail so it is drawn again, for when a slide's words change, with the
    /// pictures of it in theme versions.
    pub fn forget(&mut self, key: Uuid, egui_renderer: &mut egui_wgpu::Renderer) {
        let gone: Vec<Uuid> =
            self.cache.iter().filter(|(have, thumb)| **have == key || thumb.of == key).map(|(have, _)| *have).collect();
        for have in gone {
            if let Some(thumb) = self.cache.remove(&have) {
                egui_renderer.free_texture(&thumb.id);
            }
        }
    }

    fn get_layers(
        &mut self,
        gpu: &Gpu,
        egui_renderer: &mut egui_wgpu::Renderer,
        (key, of): (Uuid, Uuid),
        layers: &Layers,
        theme: &Theme,
    ) -> Option<(egui::TextureId, bool)> {
        if let Some(thumb) = self.cache.get_mut(&key) {
            let retry = thumb.failed.is_some_and(|at| at.elapsed() >= RETRY_AFTER);
            if !retry {
                thumb.last_used = self.frame;
                return Some((thumb.id, thumb.overflow));
            }
            if let Some(thumb) = self.cache.remove(&key) {
                egui_renderer.free_texture(&thumb.id);
            }
        }
        if self.drawn_this_frame >= PER_FRAME {
            return None;
        }
        self.drawn_this_frame += 1;
        let built = build_layers(&mut self.fonts, layers, theme, THUMB_W, THUMB_H);
        let texture = target_texture(&gpu.device, THUMB_W, THUMB_H);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        // One that can't be drawn is kept blank (a new texture is clear), so the window doesn't
        // try again, and ask to be drawn again, on every frame.
        let drawn = self.painter.paint(&gpu.device, &gpu.queue, &built.scene, &view, THUMB_W, THUMB_H);
        match &drawn {
            Ok(()) => drop(self.thumbnails_said.remove(&key)),
            Err(error) => {
                // Not without end: slides come and go.
                if self.thumbnails_said.len() >= 1000 {
                    self.thumbnails_said.clear();
                }
                if self.thumbnails_said.insert(key) {
                    crate::applog::problem("window", &format!("Could not draw a thumbnail: {error}"));
                }
            }
        }
        let failed = drawn.is_err().then(std::time::Instant::now);
        let overflow = built.overflow && failed.is_none();
        let id = egui_renderer.register_native_texture(&gpu.device, &view, wgpu::FilterMode::Linear);
        let pages = web_pages(layers.slide, theme);
        self.cache.insert(key, Thumb { id, of, overflow, last_used: self.frame, pages, failed, _texture: texture });
        let ages: Vec<(Uuid, u64)> = self.cache.iter().map(|(id, thumb)| (*id, thumb.last_used)).collect();
        for old in to_evict(&ages, MAX_CACHED, self.frame) {
            if let Some(thumb) = self.cache.remove(&old) {
                egui_renderer.free_texture(&thumb.id);
            }
        }
        Some((id, overflow))
    }

    /// Draws a picture as a thumbnail is drawn, with the same painter and fonts, into a texture of
    /// its own that the window never shows and the cache never keeps: for Remote Control, which
    /// reads it back on its own thread. `None` when it couldn't be drawn.
    pub fn draw_alone(&mut self, gpu: &Gpu, layers: &Layers, theme: &Theme) -> Option<wgpu::Texture> {
        let built = build_layers(&mut self.fonts, layers, theme, THUMB_W, THUMB_H);
        let texture = target_texture(&gpu.device, THUMB_W, THUMB_H);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.painter.paint(&gpu.device, &gpu.queue, &built.scene, &view, THUMB_W, THUMB_H).ok()?;
        Some(texture)
    }

    /// Forgets the thumbnails that show a web page box whose address `shows` says yes to, so
    /// they are drawn again (with the page's picture in place of the placeholder).
    pub fn forget_showing(&mut self, egui_renderer: &mut egui_wgpu::Renderer, shows: impl Fn(&str) -> bool) {
        let gone: Vec<Uuid> = self
            .cache
            .iter()
            .filter(|(_, thumb)| thumb.pages.iter().any(|url| shows(url)))
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            self.forget(id, egui_renderer);
        }
    }

    /// Forgets the thumbnails of slides that are no longer shown, for when another song is
    /// selected. `keep` says which ones to hold on to (the media bin's).
    pub fn clear_except(&mut self, egui_renderer: &mut egui_wgpu::Renderer, keep: impl Fn(&Uuid) -> bool) {
        let gone: Vec<Uuid> = self.cache.keys().filter(|id| !keep(id)).copied().collect();
        for id in gone {
            self.forget(id, egui_renderer);
        }
    }

    /// Redraws the Edit view's large picture of the slide being edited and returns its texture.
    pub fn canvas(
        &mut self,
        gpu: &Gpu,
        egui_renderer: &mut egui_wgpu::Renderer,
        slide: &Slide,
        theme: &Theme,
    ) -> Option<egui::TextureId> {
        // Large enough to stay sharp when the slide is zoomed in on a Retina screen.
        const W: u32 = 2560;
        const H: u32 = 1440;
        let built = build_layers(
            &mut self.fonts,
            &Layers {
                media_fit: sp_model::ImageFit::Fill,
                media: None,
                slide: Some(slide),
                overlay: None,
                time: 0.0,
                backdrop: None,
                mask: [0.0; 4],
                announcement: None,
            },
            theme,
            W,
            H,
        );
        if self.canvas.is_none() {
            let texture = target_texture(&gpu.device, W, H);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let id = egui_renderer.register_native_texture(&gpu.device, &view, wgpu::FilterMode::Linear);
            self.canvas = Some((texture, id));
        }
        let (texture, id) = self.canvas.as_ref()?;
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let id = *id;
        let drawn = self.painter.paint(&gpu.device, &gpu.queue, &built.scene, &view, W, H);
        self.note_drawn(Kind::Canvas, &drawn);
        Some(id)
    }

    /// Draws the audience screen's picture for the preview. Returns it, and the picture that
    /// was there before, which is kept so the new one can fade in over it.
    /// The preview, as [`Thumbs::preview`] makes it, with one audience screen's picture at hand
    /// for a picture of that screen in it (a stage layout showing the audience): that screen's
    /// layers are drawn first and shown in its place.
    pub fn preview_showing(
        &mut self,
        gpu: &Gpu,
        egui_renderer: &mut egui_wgpu::Renderer,
        layers: &Layers,
        theme: &Theme,
        screen: Option<(sp_model::Uuid, &Layers, &Theme)>,
    ) -> (Option<egui::TextureId>, Option<egui::TextureId>) {
        let pictures: Vec<sp_render::ScreenPicture> = screen
            .map(|(id, shown, shown_theme)| {
                sp_render::show_placeholders(false);
                let scene = build_layers(&mut self.fonts, shown, shown_theme, PREVIEW_W, PREVIEW_H).scene;
                sp_render::show_placeholders(true);
                sp_render::ScreenPicture { screen: id, scene, width: PREVIEW_W, height: PREVIEW_H }
            })
            .into_iter()
            .collect();
        sp_render::with_screen_pictures(pictures, || self.preview(gpu, egui_renderer, layers, theme))
    }

    pub fn preview(
        &mut self,
        gpu: &Gpu,
        egui_renderer: &mut egui_wgpu::Renderer,
        layers: &Layers,
        theme: &Theme,
    ) -> (Option<egui::TextureId>, Option<egui::TextureId>) {
        // The preview is what the audience sees: no placeholder for a web page not drawn yet.
        sp_render::show_placeholders(false);
        let built = build_layers(&mut self.fonts, layers, theme, PREVIEW_W, PREVIEW_H);
        sp_render::show_placeholders(true);
        let before = self.previews[self.preview_front].as_ref().map(|(_, id)| *id);
        // Draw into the picture that is not in front, then bring it to the front.
        let back = 1 - self.preview_front;
        if self.previews[back].is_none() {
            let texture = target_texture(&gpu.device, PREVIEW_W, PREVIEW_H);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let id = egui_renderer.register_native_texture(&gpu.device, &view, wgpu::FilterMode::Linear);
            self.previews[back] = Some((texture, id));
        }
        let Some((texture, id)) = self.previews[back].as_ref() else { return (before, None) };
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let id = *id;
        let drawn = self.painter.paint(&gpu.device, &gpu.queue, &built.scene, &view, PREVIEW_W, PREVIEW_H);
        self.note_drawn(Kind::Preview, &drawn);
        self.preview_front = back;
        (Some(id), before)
    }
}

/// The addresses of the web page boxes a slide shows: its own and those its layout puts
/// behind it (hidden ones too, which costs nothing but an extra redraw).
pub fn web_pages(slide: Option<&Slide>, theme: &Theme) -> Vec<String> {
    let Some(slide) = slide else { return Vec::new() };
    let behind = theme.template_for(slide).map_or(&[][..], |template| template.objects.as_slice());
    slide
        .objects
        .iter()
        .chain(behind)
        .filter_map(|object| match &object.kind {
            sp_model::ObjectKind::Web { url } => Some(url.clone()),
            // A thumbnail is drawn again when its NDI source's first picture comes.
            sp_model::ObjectKind::Ndi { source, .. } if !source.is_empty() => Some(format!("ndi:{source}")),
            _ => None,
        })
        .collect()
}

/// Which thumbnails to drop so that at most `max` remain: the ones shown longest ago, and
/// never one shown in the current frame (it is on screen right now).
fn to_evict(ages: &[(Uuid, u64)], max: usize, current_frame: u64) -> Vec<Uuid> {
    if ages.len() <= max {
        return Vec::new();
    }
    let mut old: Vec<(Uuid, u64)> = ages.iter().copied().filter(|(_, used)| *used < current_frame).collect();
    old.sort_by_key(|(_, used)| *used);
    old.into_iter().take(ages.len() - max).map(|(id, _)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ages(used: &[u64]) -> Vec<(Uuid, u64)> {
        used.iter().map(|u| (Uuid::new_v4(), *u)).collect()
    }

    #[test]
    fn nothing_is_dropped_under_the_cap() {
        assert!(to_evict(&ages(&[1, 2, 3]), 3, 9).is_empty());
        assert!(to_evict(&[], 3, 9).is_empty());
    }

    #[test]
    fn the_oldest_are_dropped_first() {
        let all = ages(&[5, 1, 9, 3]);
        let dropped = to_evict(&all, 2, 10);
        assert_eq!(dropped, vec![all[1].0, all[3].0]);
    }

    #[test]
    fn thumbnails_on_screen_now_are_kept_even_over_the_cap() {
        let all = ages(&[7, 7, 7, 2]);
        assert_eq!(to_evict(&all, 1, 7), vec![all[3].0]);
    }

    #[test]
    fn a_thumbnail_knows_the_web_pages_of_its_slide_and_of_its_layout() {
        let mut theme = Theme::builtin();
        theme.templates[0].objects.push(sp_model::Object::web("https://example.org/behind".to_owned()));
        let mut slide = Slide::new(vec![
            sp_model::Object::web("https://example.org/own".to_owned()),
            sp_model::Object::text(sp_model::Role::primary(), "Words".to_owned()),
        ]);
        slide.template = Some(theme.templates[0].id);
        assert_eq!(web_pages(Some(&slide), &theme), ["https://example.org/own", "https://example.org/behind"]);
        assert!(web_pages(None, &theme).is_empty(), "a picture in the media bin shows no page");
    }
}
