//! Turns a slide and a theme into a picture on the GPU. The same code draws outputs,
//! thumbnails and previews, so the operator sees what the audience sees. Knows nothing about
//! windows.

mod close_mark;
mod fade_blend;
mod fonts;
mod gpu;
mod picture;
mod pictures;
mod scene;
mod see_through;
mod yuv;

pub use close_mark::{CloseMark, MarkLook, Square};
pub use fade_blend::{FadeBlend, Weights, blend_weights};
pub use fonts::{Fonts, bundled_regular, copy_large_fonts_into_caches};
pub use gpu::{
    CachedSide, MovingPicture, PaintError, Painter, early_stages_on_processor, read_pixels, read_pixels_gently,
    target_texture,
};
pub use picture::{Picture, PixelOrder};
pub use pictures::{MIRROR_SOURCE, forget_mirror, keep_mirror, mirror_quiet, set_mirror_receiving, show_mirror};
pub use pictures::{
    NDI_FADE, NDI_HOLD, forget_ndi, ndi_changes, ndi_quiet, ndi_visibility, set_ndi_receiving, show_ndi,
};
pub use pictures::{
    draw_kept_only, forget_picture, lock_waits, read_ahead, set_picture_loader, set_small_picture_loader, show_picture,
    web_key,
};
pub use scene::{
    Built, EditorBackdrop, Layers, Parts, ScreenPicture, build_layer_parts, build_layers, build_layers_for_editor,
    build_overlay, build_scene, build_under, cross_fade, fade_cacheable, fit_rect, layout_objects, object_frame,
    plain_black, scrolls, set_ndi_tag_words, set_web_stand_in, shade_empty_boxes, show_placeholders,
    take_end_mark_left_out, transition, with_screen_pictures,
};
pub use see_through::{Solid, SolidPass, see_through_transition};
pub use vello;
pub use vello::wgpu;
pub use yuv::{PlanesWork, YuvColours, YuvMatrix, YuvPlanes};
