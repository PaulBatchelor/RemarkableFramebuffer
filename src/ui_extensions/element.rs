use std;
use std::sync::{Arc, RwLock};
use std::hash::{Hash, Hasher};

use image;

use framebuffer::common;
use framebuffer::FramebufferRefresh;
use framebuffer::refresh::PartialRefreshMode;
use framebuffer::FramebufferDraw;
use framebuffer::common::{color, mxcfb_rect};

use appctx;

pub type ActiveRegionFunction = fn(&mut appctx::ApplicationContext, Arc<RwLock<UIElementWrapper>>);

#[derive(Clone)]
pub struct ActiveRegionHandler {
    pub handler: ActiveRegionFunction,
    pub element: Arc<RwLock<UIElementWrapper>>,
}

impl<'a> std::fmt::Debug for ActiveRegionHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{0:p}", self)
    }
}

#[derive(Clone)]
pub enum UIConstraintRefresh {
    NoRefresh,
    Refresh,
    RefreshAndWait,
}

impl Default for UIConstraintRefresh {
    fn default() -> UIConstraintRefresh {
        UIConstraintRefresh::Refresh
    }
}

#[derive(Clone, Default)]
pub struct UIElementWrapper {
    pub y: usize,
    pub x: usize,
    pub refresh: UIConstraintRefresh,
    pub last_drawn_rect: Option<common::mxcfb_rect>,
    pub onclick: Option<ActiveRegionFunction>,
    pub inner: UIElement,
}

impl Hash for UIElementWrapper {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.x.hash(state);
        self.y.hash(state);
    }
}

impl PartialEq for UIElementWrapper {
    fn eq(&self, other: &UIElementWrapper) -> bool {
        self.x == other.x && self.y == other.y
    }
}

impl Eq for UIElementWrapper {}

#[derive(Clone)]
pub enum UIElement {
    Text {
        text: String,
        scale: usize,
        foreground: color,
    },
    Image {
        img: image::DynamicImage,
    },
    Unspecified,
}

impl UIElementWrapper {
    pub fn draw(
        &mut self,
        app: &mut appctx::ApplicationContext,
        handler: Option<ActiveRegionHandler>,
    ) {
        let (x, y) = (self.x, self.y);
        let refresh = self.refresh.clone();
        let framebuffer = app.get_framebuffer_ref();

        let old_filled_rect = match self.last_drawn_rect {
            Some(rect) => {
                // Clear the background on the last occupied region
                framebuffer.fill_rect(
                    rect.top as usize,
                    rect.left as usize,
                    rect.height as usize,
                    rect.width as usize,
                    color::WHITE,
                );

                // We have filled the old_filled_rect, now we need to also refresh that but if
                // only if it isn't at the same spot. Otherwise we will be refreshing it for no
                // reason and showing a blank frame. There is of course still a caveat since we don't
                // know the dimensions of a drawn text before it is actually drawn.
                // TODO: Take care of the point above ^
                if rect.top != y as u32 && rect.left != x as u32 {
                    framebuffer.partial_refresh(
                        &rect,
                        PartialRefreshMode::Wait,
                        common::waveform_mode::WAVEFORM_MODE_DU,
                        common::display_temp::TEMP_USE_REMARKABLE_DRAW,
                        common::dither_mode::EPDC_FLAG_USE_DITHERING_PASSTHROUGH,
                        0,
                    );
                }

                rect
            }
            None => mxcfb_rect::invalid(),
        };

        // TODO: Move this to inside the app and then have it call the UIElement's draw
        let rect = match self.inner {
            UIElement::Text {
                ref text,
                scale,
                foreground,
            } => app.display_text(y, x, foreground, scale, text.to_string(), refresh),
            UIElement::Image { ref img } => app.display_image(&img, y, x, refresh),
            UIElement::Unspecified => return,
        };

        // If no changes, no need to change the active region
        if old_filled_rect != rect {
            if let Some(ref h) = handler {
                if old_filled_rect != mxcfb_rect::invalid() {
                    app.remove_active_region_at_point(
                        old_filled_rect.top as u16,
                        old_filled_rect.left as u16,
                    );
                }

                if app.find_active_region(y as u16, x as u16).is_none() {
                    app.create_active_region(
                        rect.top as u16,
                        rect.left as u16,
                        rect.height as u16,
                        rect.width as u16,
                        h.handler,
                        Arc::clone(&h.element),
                    );
                }
            }
        }

        // We need to wait until now because we don't know the size of the active region before we
        // actually go ahead and draw it.
        self.last_drawn_rect = Some(rect);
    }
}

impl Default for UIElement {
    fn default() -> UIElement {
        UIElement::Unspecified
    }
}
