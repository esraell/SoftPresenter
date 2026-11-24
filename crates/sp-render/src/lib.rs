//! Turns a slide and a theme into a picture on the GPU. The same code draws outputs,
//! thumbnails and previews, so the operator sees what the audience sees. Knows nothing about
//! windows.

mod fonts;
mod gpu;
mod picture;
mod pictures;
mod scene;
mod see_through;

pub use fonts::{Fonts, copy_large_fonts_into_caches};
pub use gpu::{MovingPicture, PaintError, Painter, read_pixels, read_pixels_gently, target_texture};
pub use picture::{Picture, PixelOrder};
pub use pictures::{
    NDI_FADE, NDI_HOLD, forget_ndi, ndi_changes, ndi_quiet, ndi_visibility, set_ndi_receiving, show_ndi,
};
pub use pictures::{
    draw_kept_only, forget_picture, lock_waits, read_ahead, set_picture_loader, set_small_picture_loader, show_picture,
    web_key,
};
pub use scene::{
    Built, Layers, ScreenPicture, build_layers, build_overlay, build_scene, cross_fade, fit_rect, layout_objects,
    object_frame, scrolls, set_web_stand_in, show_placeholders, transition, with_screen_pictures,
};
pub use see_through::{Solid, SolidPass, see_through_transition};
pub use vello;
pub use vello::wgpu;
