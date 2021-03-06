use libc;

use std::os::unix::io::AsRawFd;
use std::sync::atomic::Ordering;

use framebuffer;
use framebuffer::common;
use framebuffer::core;
use framebuffer::mxcfb::*;

macro_rules! max {
        ($x: expr) => ($x);
        ($x: expr, $($z: expr),+) => (::std::cmp::max($x, max!($($z),*)));
}

/// The minimum height/width that we will enforce before each call to MXCFB_SEND_UPDATE
/// The higher it is, the more likely we are to have collisions between updates.
/// The smaller it is, the more likely we are to have display artifacts.
/// 16 or 32 also seems like a decent minimum as this accelerates the initial processing,
/// and therefore minimizing collisions through a different mechanism.
const MIN_SEND_UPDATE_DIMENSION_PX: u32 = 32;

pub enum PartialRefreshMode {
    DryRun,
    Async,
    Wait,
}

impl<'a> framebuffer::FramebufferRefresh for core::Framebuffer<'a> {
    fn full_refresh(
        &mut self,
        waveform_mode: common::waveform_mode,
        temperature: common::display_temp,
        dither_mode: common::dither_mode,
        quant_bit: i32,
        wait_completion: bool,
    ) -> u32 {
        let screen = common::mxcfb_rect {
            top: 0,
            left: 0,
            height: self.var_screen_info.yres,
            width: self.var_screen_info.xres,
        };
        let whole = mxcfb_update_data {
            update_mode: common::update_mode::UPDATE_MODE_FULL as u32,
            update_marker: *self.marker.get_mut() as u32,
            waveform_mode: waveform_mode as u32,
            temp: temperature as i32,
            flags: 0,
            quant_bit,
            dither_mode: dither_mode as i32,
            update_region: screen,
            ..Default::default()
        };
        self.marker.swap(whole.update_marker + 1, Ordering::Relaxed);

        let pt: *const mxcfb_update_data = &whole;
        unsafe {
            libc::ioctl(self.device.as_raw_fd(), common::MXCFB_SEND_UPDATE, pt);
        }

        if wait_completion {
            let mut markerdata = mxcfb_update_marker_data {
                update_marker: whole.update_marker,
                collision_test: 0,
            };
            unsafe {
                if libc::ioctl(
                    self.device.as_raw_fd(),
                    common::MXCFB_WAIT_FOR_UPDATE_COMPLETE,
                    &mut markerdata,
                ) < 0
                {
                    warn!("WAIT_FOR_UPDATE_COMPLETE failed after a full_refresh(..)");
                }
            }
        }
        whole.update_marker
    }

    fn partial_refresh(
        &mut self,
        region: &common::mxcfb_rect,
        mode: PartialRefreshMode,
        waveform_mode: common::waveform_mode,
        temperature: common::display_temp,
        dither_mode: common::dither_mode,
        quant_bit: i32,
    ) -> u32 {
        let mut update_region = region.clone();

        // No accounting for this, out of bounds, entirely ignored
        if update_region.left >= common::DISPLAYWIDTH as u32
            || update_region.top >= common::DISPLAYHEIGHT as u32
        {
            return 0;
        }

        update_region.width = max!(update_region.width, MIN_SEND_UPDATE_DIMENSION_PX);
        update_region.height = max!(update_region.height, MIN_SEND_UPDATE_DIMENSION_PX);

        // Dont try to refresh OOB horizontally
        let max_x = update_region.left + update_region.width;
        if max_x > common::DISPLAYWIDTH as u32 {
            update_region.width -= max_x - (common::DISPLAYWIDTH as u32);
        }

        // Dont try to refresh OOB vertically
        let max_y = update_region.top + update_region.height;
        if max_y > common::DISPLAYHEIGHT as u32 {
            update_region.height -= max_y - (common::DISPLAYHEIGHT as u32);
        }

        let whole = mxcfb_update_data {
            update_mode: common::update_mode::UPDATE_MODE_PARTIAL as u32,
            update_marker: *self.marker.get_mut() as u32,
            waveform_mode: waveform_mode as u32,
            temp: temperature as i32,
            flags: match mode {
                PartialRefreshMode::DryRun => common::EPDC_FLAG_TEST_COLLISION as u32,
                _ => 0,
            },
            quant_bit,
            dither_mode: dither_mode as i32,
            update_region,
            ..Default::default()
        };
        self.marker.swap(whole.update_marker + 1, Ordering::Relaxed);

        let pt: *const mxcfb_update_data = &whole;
        unsafe {
            libc::ioctl(self.device.as_raw_fd(), common::MXCFB_SEND_UPDATE, pt);
        }

        match mode {
            PartialRefreshMode::Wait | PartialRefreshMode::DryRun => {
                let mut markerdata = mxcfb_update_marker_data {
                    update_marker: whole.update_marker,
                    collision_test: 0,
                };
                unsafe {
                    if libc::ioctl(
                        self.device.as_raw_fd(),
                        common::MXCFB_WAIT_FOR_UPDATE_COMPLETE,
                        &mut markerdata,
                    ) < 0
                    {
                        warn!("WAIT_FOR_UPDATE_COMPLETE failed after a partial_refresh(..)");
                    }
                }
                markerdata.collision_test
            }
            PartialRefreshMode::Async => whole.update_marker,
        }
    }

    fn wait_refresh_complete(&mut self, marker: u32) -> u32 {
        let mut markerdata = mxcfb_update_marker_data {
            update_marker: marker,
            collision_test: 0,
        };
        unsafe {
            if libc::ioctl(
                self.device.as_raw_fd(),
                common::MXCFB_WAIT_FOR_UPDATE_COMPLETE,
                &mut markerdata,
            ) < 0
            {
                warn!("WAIT_FOR_UPDATE_COMPLETE failed");
            }
        };
        return markerdata.collision_test;
    }
}
