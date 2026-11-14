//! Platform-agnostic unified input router for mouse, keyboard, and touch.
//!
//! Translates raw window-space pointer, touch, and key events into [`AppInput`]
//! snapshots while handling:
//! - Logical/physical/DOS coordinate mapping and letterboxing.
//! - Synthetic mouse-from-touch deduplication (`SDL_TOUCH_MOUSEID`).
//! - Explicit primary/secondary touch contact ownership.
//! - Touch right-click alternatives (long-press and two-finger tap).
//! - Two-finger vertical drag for tray scrolling.
//! - Clean transitions between keyboard, mouse, and touch without stale state.

use crate::app::{AppInput, ViewportRect, game_viewport};
use crate::graphics::{SCREEN_H, SCREEN_W};

/// Active input modality, used to control cursor visibility and stale-state resets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputDeviceKind {
    Mouse,
    Touch,
    Keyboard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKey {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Space,
    Escape,
    Tab,
    W,
    A,
    S,
    D,
    E,
    R,
    G,
    P,
    Q,
    T,
    M,
    H,
    F1,
    F2,
    F3,
    F4,
    F5,
    F7,
    F8,
}

/// Platform-agnostic input events fed from the windowing backend (SDL3 or tests).
///
/// All `(x, y)` coordinates are in window-logical points (matching `window.size()`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlatformEvent {
    MouseMove {
        x: f32,
        y: f32,
        is_touch_synth: bool,
    },
    MouseDown {
        button: PointerButton,
        x: f32,
        y: f32,
        is_touch_synth: bool,
    },
    MouseUp {
        button: PointerButton,
        x: f32,
        y: f32,
        is_touch_synth: bool,
    },
    MouseWheel {
        delta_y: f32,
        is_touch_synth: bool,
    },
    TouchDown {
        finger_id: u64,
        x: f32,
        y: f32,
    },
    TouchMove {
        finger_id: u64,
        x: f32,
        y: f32,
    },
    TouchUp {
        finger_id: u64,
        x: f32,
        y: f32,
    },
    TouchCancel {
        finger_id: u64,
    },
    KeyDown {
        key: InputKey,
        repeat: bool,
    },
    KeyUp {
        key: InputKey,
    },
    WindowFocusLost,
}

/// Maps between window-logical coordinates, physical pixels, and 320x200 DOS coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportTransform {
    pub win_w: f32,
    pub win_h: f32,
    pub pixel_density: f32,
    pub viewport: ViewportRect,
    pub scale: f32,
}

impl ViewportTransform {
    pub fn new(win_w: f32, win_h: f32, pixel_density: f32, playing: bool) -> Self {
        let win_w = win_w.max(1.0);
        let win_h = win_h.max(1.0);
        let pixel_density = if pixel_density.is_finite() && pixel_density > 0.0 {
            pixel_density
        } else {
            1.0
        };
        let viewport = game_viewport(win_w, win_h, playing);
        let scale = (viewport.w / SCREEN_W as f32).max(f32::EPSILON);
        Self {
            win_w,
            win_h,
            pixel_density,
            viewport,
            scale,
        }
    }

    pub fn physical_size(&self) -> (u32, u32) {
        (
            (self.win_w * self.pixel_density).round().max(1.0) as u32,
            (self.win_h * self.pixel_density).round().max(1.0) as u32,
        )
    }

    pub fn physical_to_logical(&self, px: f32, py: f32) -> (f32, f32) {
        (px / self.pixel_density, py / self.pixel_density)
    }

    pub fn logical_to_dos(&self, lx: f32, ly: f32) -> (i32, i32, bool) {
        let dx = ((lx - self.viewport.x) / self.scale).floor() as i32;
        let dy = ((ly - self.viewport.y) / self.scale).floor() as i32;
        let inside = (0..SCREEN_W).contains(&dx) && (0..SCREEN_H).contains(&dy);
        (dx, dy, inside)
    }

    pub fn hud_visible(&self, playing: bool) -> bool {
        playing && self.win_w >= 480.0 && self.win_h >= 360.0
    }

    pub fn reference_button_rect(&self) -> ViewportRect {
        ViewportRect {
            x: self.win_w - 210.0,
            y: 42.0,
            w: 194.0,
            h: 30.0,
        }
    }

    pub fn point_in_reference_button(&self, lx: f32, ly: f32, playing: bool) -> bool {
        if !self.hud_visible(playing) {
            return false;
        }
        let r = self.reference_button_rect();
        lx >= r.x && lx < r.x + r.w && ly >= r.y && ly < r.y + r.h
    }
}

const TOUCH_TAP_SLOP_PX: f32 = 8.0;
const TOUCH_LONG_PRESS_SECS: f32 = 0.45;
const TOUCH_TWO_FINGER_TAP_MAX_SECS: f32 = 0.30;
const TOUCH_SCROLL_STEP_PX: f32 = 24.0;

#[derive(Clone, Copy, Debug)]
struct PrimaryTouch {
    finger_id: u64,
    start_x: f32,
    start_y: f32,
    x: f32,
    y: f32,
    held_secs: f32,
    moved_beyond_slop: bool,
    left_committed: bool,
    long_press_fired: bool,
    suppressed: bool,
}

#[derive(Clone, Copy, Debug)]
struct SecondaryTouch {
    finger_id: u64,
    start_x: f32,
    start_y: f32,
    last_y: f32,
    held_secs: f32,
    accum_scroll_y: f32,
    did_scroll: bool,
    moved_beyond_slop: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct HeldKeys {
    w: bool,
    a: bool,
    s: bool,
    d: bool,
    tab: bool,
}

/// Per-frame output produced by [`InputRouter::finish_frame`].
#[derive(Clone, Copy, Debug)]
pub struct FrameInput {
    pub app_input: AppInput,
    pub show_reference: bool,
    pub toggle_picture_guide: bool,
    pub hud_ref_hovered: bool,
    pub active_device: InputDeviceKind,
}

/// Stateful unified input router translating [`PlatformEvent`]s into [`FrameInput`].
pub struct InputRouter {
    active_device: InputDeviceKind,
    pointer_logical: (f32, f32),
    last_dos_pointer: (i32, i32),
    mouse_left_down: bool,
    hud_ref_pointer_down: bool,
    primary_touch: Option<PrimaryTouch>,
    secondary_touch: Option<SecondaryTouch>,
    deferred_left_release: Option<(f32, f32)>,
    held_keys: HeldKeys,
    pending_events: Vec<PlatformEvent>,
}

impl Default for InputRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl InputRouter {
    pub fn new() -> Self {
        Self {
            active_device: InputDeviceKind::Mouse,
            pointer_logical: (-1.0, -1.0),
            last_dos_pointer: (-1, -1),
            mouse_left_down: false,
            hud_ref_pointer_down: false,
            primary_touch: None,
            secondary_touch: None,
            deferred_left_release: None,
            held_keys: HeldKeys::default(),
            pending_events: Vec::with_capacity(16),
        }
    }

    pub fn active_device(&self) -> InputDeviceKind {
        self.active_device
    }

    pub fn pointer_logical(&self) -> (f32, f32) {
        self.pointer_logical
    }

    pub fn push_event(&mut self, event: PlatformEvent) {
        self.pending_events.push(event);
    }

    fn set_key(&mut self, key: InputKey, down: bool) {
        match key {
            InputKey::W => self.held_keys.w = down,
            InputKey::A => self.held_keys.a = down,
            InputKey::S => self.held_keys.s = down,
            InputKey::D => self.held_keys.d = down,
            InputKey::Tab => self.held_keys.tab = down,
            _ => {}
        }
    }

    fn cancel_touch_contacts(&mut self) {
        self.primary_touch = None;
        self.secondary_touch = None;
        self.deferred_left_release = None;
        self.hud_ref_pointer_down = false;
    }

    /// Processes all queued [`PlatformEvent`]s for the current frame and returns [`FrameInput`].
    pub fn finish_frame(
        &mut self,
        dt: f32,
        transform: &ViewportTransform,
        playing: bool,
        reference_available: bool,
    ) -> FrameInput {
        let mut input = AppInput {
            dt,
            ..Default::default()
        };
        let mut toggle_picture_guide = false;
        let mut wheel_accum = 0.0f32;
        let mut prev_logical = self.pointer_logical;
        let mut override_dos_logical: Option<(f32, f32)> = None;

        if let Some((rx, ry)) = self.deferred_left_release.take() {
            input.left_released = true;
            override_dos_logical = Some((rx, ry));
        }

        let events = std::mem::take(&mut self.pending_events);
        for ev in events {
            match ev {
                PlatformEvent::MouseMove {
                    x,
                    y,
                    is_touch_synth,
                } => {
                    if is_touch_synth {
                        continue;
                    }
                    if self.active_device == InputDeviceKind::Touch {
                        self.cancel_touch_contacts();
                    }
                    self.active_device = InputDeviceKind::Mouse;
                    self.pointer_logical = (x, y);
                }
                PlatformEvent::MouseDown {
                    button,
                    x,
                    y,
                    is_touch_synth,
                } => {
                    if is_touch_synth {
                        continue;
                    }
                    if self.active_device == InputDeviceKind::Touch {
                        self.cancel_touch_contacts();
                    }
                    self.active_device = InputDeviceKind::Mouse;
                    self.pointer_logical = (x, y);
                    let (_, _, inside) = transform.logical_to_dos(x, y);
                    match button {
                        PointerButton::Left => {
                            self.mouse_left_down = true;
                            if reference_available
                                && transform.point_in_reference_button(x, y, playing)
                            {
                                self.hud_ref_pointer_down = true;
                            } else if inside {
                                input.left_pressed = true;
                            }
                        }
                        PointerButton::Right => {
                            if inside {
                                input.right_pressed = true;
                            }
                        }
                    }
                }
                PlatformEvent::MouseUp {
                    button,
                    x,
                    y,
                    is_touch_synth,
                } => {
                    if is_touch_synth {
                        continue;
                    }
                    self.active_device = InputDeviceKind::Mouse;
                    self.pointer_logical = (x, y);
                    if button == PointerButton::Left {
                        self.mouse_left_down = false;
                        if self.hud_ref_pointer_down {
                            self.hud_ref_pointer_down = false;
                        } else {
                            input.left_released = true;
                        }
                    }
                }
                PlatformEvent::MouseWheel {
                    delta_y,
                    is_touch_synth,
                } => {
                    if is_touch_synth {
                        continue;
                    }
                    wheel_accum += delta_y;
                }
                PlatformEvent::TouchDown { finger_id, x, y } => {
                    self.active_device = InputDeviceKind::Touch;
                    self.mouse_left_down = false;
                    if self.primary_touch.is_none() {
                        self.pointer_logical = (x, y);
                        prev_logical = (x, y);
                        self.primary_touch = Some(PrimaryTouch {
                            finger_id,
                            start_x: x,
                            start_y: y,
                            x,
                            y,
                            held_secs: 0.0,
                            moved_beyond_slop: false,
                            left_committed: false,
                            long_press_fired: false,
                            suppressed: false,
                        });
                        if reference_available && transform.point_in_reference_button(x, y, playing)
                        {
                            self.hud_ref_pointer_down = true;
                        }
                    } else if self.secondary_touch.is_none() {
                        if let Some(ref mut p) = self.primary_touch
                            && !p.left_committed
                        {
                            p.suppressed = true;
                        }
                        self.secondary_touch = Some(SecondaryTouch {
                            finger_id,
                            start_x: x,
                            start_y: y,
                            last_y: y,
                            held_secs: 0.0,
                            accum_scroll_y: 0.0,
                            did_scroll: false,
                            moved_beyond_slop: false,
                        });
                    }
                }
                PlatformEvent::TouchMove { finger_id, x, y } => {
                    if let Some(ref mut p) = self.primary_touch
                        && p.finger_id == finger_id
                    {
                        p.x = x;
                        p.y = y;
                        self.pointer_logical = (x, y);
                        let dist_sq = (x - p.start_x).powi(2) + (y - p.start_y).powi(2);
                        if dist_sq > TOUCH_TAP_SLOP_PX * TOUCH_TAP_SLOP_PX {
                            p.moved_beyond_slop = true;
                            if !p.left_committed
                                && !p.long_press_fired
                                && !p.suppressed
                                && !self.hud_ref_pointer_down
                                && self.secondary_touch.is_none()
                            {
                                let (_, _, inside) = transform.logical_to_dos(p.start_x, p.start_y);
                                if inside {
                                    p.left_committed = true;
                                    input.left_pressed = true;
                                    override_dos_logical = Some((p.start_x, p.start_y));
                                }
                            }
                        }
                    } else if let Some(ref mut s) = self.secondary_touch
                        && s.finger_id == finger_id
                    {
                        let dy = y - s.last_y;
                        s.last_y = y;
                        let dist_sq = (x - s.start_x).powi(2) + (y - s.start_y).powi(2);
                        if dist_sq > TOUCH_TAP_SLOP_PX * TOUCH_TAP_SLOP_PX {
                            s.moved_beyond_slop = true;
                        }
                        s.accum_scroll_y += dy;
                        while s.accum_scroll_y >= TOUCH_SCROLL_STEP_PX {
                            s.accum_scroll_y -= TOUCH_SCROLL_STEP_PX;
                            input.wheel_delta += 1;
                            s.did_scroll = true;
                        }
                        while s.accum_scroll_y <= -TOUCH_SCROLL_STEP_PX {
                            s.accum_scroll_y += TOUCH_SCROLL_STEP_PX;
                            input.wheel_delta -= 1;
                            s.did_scroll = true;
                        }
                    }
                }
                PlatformEvent::TouchUp { finger_id, x, y } => {
                    if let Some(p) = self.primary_touch
                        && p.finger_id == finger_id
                    {
                        self.pointer_logical = (x, y);
                        self.primary_touch = None;
                        if self.hud_ref_pointer_down {
                            self.hud_ref_pointer_down = false;
                        } else if p.left_committed {
                            input.left_released = true;
                        } else if !p.long_press_fired && !p.suppressed {
                            let (_, _, inside) = transform.logical_to_dos(p.start_x, p.start_y);
                            if inside {
                                input.left_pressed = true;
                                override_dos_logical = Some((p.start_x, p.start_y));
                                self.deferred_left_release = Some((x, y));
                            }
                        }
                    } else if let Some(s) = self.secondary_touch
                        && s.finger_id == finger_id
                    {
                        self.secondary_touch = None;
                        if !s.did_scroll
                            && !s.moved_beyond_slop
                            && s.held_secs <= TOUCH_TWO_FINGER_TAP_MAX_SECS
                            && !self.hud_ref_pointer_down
                            && !self.primary_touch.is_some_and(|p| p.left_committed)
                        {
                            let (_, _, inside) = transform
                                .logical_to_dos(self.pointer_logical.0, self.pointer_logical.1);
                            if inside {
                                input.right_pressed = true;
                            }
                        }
                    }
                }
                PlatformEvent::TouchCancel { finger_id } => {
                    if self.primary_touch.is_some_and(|p| p.finger_id == finger_id) {
                        self.primary_touch = None;
                        self.hud_ref_pointer_down = false;
                        input.pointer_canceled = true;
                    } else if self
                        .secondary_touch
                        .is_some_and(|s| s.finger_id == finger_id)
                    {
                        self.secondary_touch = None;
                    }
                }
                PlatformEvent::KeyDown { key, repeat } => {
                    self.set_key(key, true);
                    if matches!(key, InputKey::W | InputKey::A | InputKey::S | InputKey::D) {
                        self.active_device = InputDeviceKind::Keyboard;
                    }
                    if !repeat {
                        match key {
                            InputKey::Up => input.up = true,
                            InputKey::Down => input.down = true,
                            InputKey::Left => input.left = true,
                            InputKey::Right => input.right = true,
                            InputKey::Enter => input.enter = true,
                            InputKey::Space => input.space = true,
                            InputKey::Escape => input.escape = true,
                            InputKey::Tab => input.tab = true,
                            InputKey::E => input.key_e = true,
                            InputKey::R => input.key_r = true,
                            InputKey::S => input.key_s = true,
                            InputKey::G => input.key_g = true,
                            InputKey::P => input.key_p = true,
                            InputKey::Q => input.key_q = true,
                            InputKey::T => input.key_t = true,
                            InputKey::M => input.key_m = true,
                            InputKey::H => {
                                if playing {
                                    toggle_picture_guide = true;
                                }
                            }
                            InputKey::F1 => input.f1 = true,
                            InputKey::F2 => input.f2 = true,
                            InputKey::F3 => input.f3 = true,
                            InputKey::F4 => input.f4 = true,
                            InputKey::F5 => input.f5 = true,
                            InputKey::F7 => input.f7 = true,
                            InputKey::F8 => input.f8 = true,
                            InputKey::W | InputKey::A | InputKey::D => {}
                        }
                    }
                }
                PlatformEvent::KeyUp { key } => {
                    self.set_key(key, false);
                }
                PlatformEvent::WindowFocusLost => {
                    self.mouse_left_down = false;
                    self.held_keys = HeldKeys::default();
                    self.cancel_touch_contacts();
                    input.pointer_canceled = true;
                }
            }
        }

        // Advance touch timers for long-press right-click & two-finger tap window
        if let Some(ref mut p) = self.primary_touch {
            p.held_secs += dt;
            if playing
                && self.secondary_touch.is_none()
                && !self.hud_ref_pointer_down
                && !p.left_committed
                && !p.suppressed
                && !p.moved_beyond_slop
                && !p.long_press_fired
                && p.held_secs >= TOUCH_LONG_PRESS_SECS
            {
                let (_, _, inside) = transform.logical_to_dos(p.x, p.y);
                if inside {
                    input.right_pressed = true;
                    p.long_press_fired = true;
                }
            }
        }
        if let Some(ref mut s) = self.secondary_touch {
            s.held_secs += dt;
        }

        if wheel_accum > 0.0 {
            input.wheel_delta += 1;
        } else if wheel_accum < 0.0 {
            input.wheel_delta -= 1;
        }

        input.move_x = i32::from(self.held_keys.d) - i32::from(self.held_keys.a);
        input.move_y = i32::from(self.held_keys.s) - i32::from(self.held_keys.w);

        let dos_logical = override_dos_logical.unwrap_or(self.pointer_logical);
        let (dos_x, dos_y, inside) = transform.logical_to_dos(dos_logical.0, dos_logical.1);
        input.pointer_x = dos_x;
        input.pointer_y = dos_y;
        input.pointer_moved = inside
            && (self.pointer_logical != prev_logical || (dos_x, dos_y) != self.last_dos_pointer);
        if inside {
            self.last_dos_pointer = (dos_x, dos_y);
        }

        let hud_ref_hovered = transform.point_in_reference_button(
            self.pointer_logical.0,
            self.pointer_logical.1,
            playing,
        );
        let show_reference =
            reference_available && (self.held_keys.tab || self.hud_ref_pointer_down);

        FrameInput {
            app_input: input,
            show_reference,
            toggle_picture_guide,
            hud_ref_hovered,
            active_device: self.active_device,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_mapping_letterboxing_and_high_dpi() {
        // 16:9 widescreen 1280x720 at 1.5x pixel density while playing (with HUD)
        let tf = ViewportTransform::new(1280.0, 720.0, 1.5, true);
        assert_eq!(tf.physical_size(), (1920, 1080));
        assert!(tf.hud_visible(true));

        let (lx, ly) = tf.physical_to_logical(tf.viewport.x * 1.5, tf.viewport.y * 1.5);
        assert!((lx - tf.viewport.x).abs() < 0.01);
        assert!((ly - tf.viewport.y).abs() < 0.01);

        let (dx, dy, inside) = tf.logical_to_dos(tf.viewport.x + 1.0, tf.viewport.y + 1.0);
        assert!(inside);
        assert_eq!((dx, dy), (0, 0));

        // Left letterbox bar should map outside 0..320
        let (dx_out, _, inside_out) = tf.logical_to_dos(tf.viewport.x - 10.0, tf.viewport.y + 10.0);
        assert!(!inside_out);
        assert!(dx_out < 0);

        // Compact window hides HUD
        let compact = ViewportTransform::new(400.0, 300.0, 1.0, true);
        assert!(!compact.hud_visible(true));
        assert_eq!(compact.viewport.x, 0.0);
    }

    #[test]
    fn synthetic_mouse_events_from_touch_are_deduplicated() {
        let tf = ViewportTransform::new(960.0, 720.0, 1.0, true);
        let mut router = InputRouter::new();
        let cx = tf.viewport.x + tf.viewport.w * 0.5;
        let cy = tf.viewport.y + tf.viewport.h * 0.5;

        router.push_event(PlatformEvent::TouchDown {
            finger_id: 1,
            x: cx,
            y: cy,
        });
        router.push_event(PlatformEvent::MouseDown {
            button: PointerButton::Left,
            x: cx,
            y: cy,
            is_touch_synth: true,
        });
        router.push_event(PlatformEvent::MouseUp {
            button: PointerButton::Left,
            x: cx,
            y: cy,
            is_touch_synth: true,
        });

        let frame = router.finish_frame(0.016, &tf, true, true);
        assert_eq!(frame.active_device, InputDeviceKind::Touch);
        assert!(!frame.app_input.left_pressed);
        assert!(!frame.app_input.left_released);

        router.push_event(PlatformEvent::TouchUp {
            finger_id: 1,
            x: cx,
            y: cy,
        });
        let frame_tap = router.finish_frame(0.016, &tf, true, true);
        assert!(frame_tap.app_input.left_pressed);
        assert!(!frame_tap.app_input.left_released);

        let frame_rel = router.finish_frame(0.016, &tf, true, true);
        assert!(frame_rel.app_input.left_released);
    }

    #[test]
    fn touch_down_move_up_and_cancel_ownership() {
        let tf = ViewportTransform::new(960.0, 720.0, 1.0, true);
        let mut router = InputRouter::new();
        let x0 = tf.viewport.x + 100.0;
        let y0 = tf.viewport.y + 100.0;

        router.push_event(PlatformEvent::TouchDown {
            finger_id: 10,
            x: x0,
            y: y0,
        });
        // Second finger move must not hijack primary pointer coordinates
        router.push_event(PlatformEvent::TouchDown {
            finger_id: 20,
            x: x0 + 200.0,
            y: y0 + 200.0,
        });
        router.push_event(PlatformEvent::TouchMove {
            finger_id: 20,
            x: x0 + 210.0,
            y: y0 + 210.0,
        });
        let f1 = router.finish_frame(0.016, &tf, true, true);
        assert!(!f1.app_input.left_pressed);
        let (exp_x, exp_y, _) = tf.logical_to_dos(x0, y0);
        assert_eq!(
            (f1.app_input.pointer_x, f1.app_input.pointer_y),
            (exp_x, exp_y)
        );

        // Cancel primary finger and lift secondary finger
        router.push_event(PlatformEvent::TouchCancel { finger_id: 10 });
        router.push_event(PlatformEvent::TouchUp {
            finger_id: 20,
            x: x0 + 210.0,
            y: y0 + 210.0,
        });
        let f2 = router.finish_frame(0.016, &tf, true, true);
        assert!(f2.app_input.pointer_canceled);
        assert!(!f2.app_input.left_released);

        // Single-finger drag beyond slop commits left_pressed at start position
        router.push_event(PlatformEvent::TouchDown {
            finger_id: 11,
            x: x0,
            y: y0,
        });
        let f_down = router.finish_frame(0.016, &tf, true, true);
        assert!(!f_down.app_input.left_pressed);
        router.push_event(PlatformEvent::TouchMove {
            finger_id: 11,
            x: x0 + 12.0,
            y: y0,
        });
        let f_drag = router.finish_frame(0.016, &tf, true, true);
        assert!(f_drag.app_input.left_pressed);
        assert_eq!(
            (f_drag.app_input.pointer_x, f_drag.app_input.pointer_y),
            (exp_x, exp_y)
        );
        router.push_event(PlatformEvent::TouchUp {
            finger_id: 11,
            x: x0 + 12.0,
            y: y0,
        });
        let f_up = router.finish_frame(0.016, &tf, true, true);
        assert!(f_up.app_input.left_released);
    }

    #[test]
    fn touch_long_press_and_two_finger_tap_and_scroll() {
        let tf = ViewportTransform::new(960.0, 720.0, 1.0, true);
        let mut router = InputRouter::new();
        let x0 = tf.viewport.x + tf.viewport.w * 0.5;
        let y0 = tf.viewport.y + tf.viewport.h * 0.5;

        // Long press triggers right_pressed without prior left_pressed and suppresses left_released on lift
        router.push_event(PlatformEvent::TouchDown {
            finger_id: 1,
            x: x0,
            y: y0,
        });
        let f_down = router.finish_frame(0.1, &tf, true, true);
        assert!(!f_down.app_input.left_pressed);
        let f_lp = router.finish_frame(0.4, &tf, true, true);
        assert!(f_lp.app_input.right_pressed);
        router.push_event(PlatformEvent::TouchUp {
            finger_id: 1,
            x: x0,
            y: y0,
        });
        let f_up = router.finish_frame(0.016, &tf, true, true);
        assert!(!f_up.app_input.left_pressed);
        assert!(!f_up.app_input.left_released);

        // Two-finger vertical drag scrolls tray
        router.push_event(PlatformEvent::TouchDown {
            finger_id: 2,
            x: x0,
            y: y0,
        });
        router.push_event(PlatformEvent::TouchDown {
            finger_id: 3,
            x: x0 + 30.0,
            y: y0,
        });
        router.push_event(PlatformEvent::TouchMove {
            finger_id: 3,
            x: x0 + 30.0,
            y: y0 - 30.0,
        });
        let f_scroll = router.finish_frame(0.016, &tf, true, true);
        assert_eq!(f_scroll.app_input.wheel_delta, -1);
    }

    #[test]
    fn device_switching_clears_stale_state() {
        let tf = ViewportTransform::new(960.0, 720.0, 1.0, true);
        let mut router = InputRouter::new();
        let btn = tf.reference_button_rect();

        // Touch down on reference button
        router.push_event(PlatformEvent::TouchDown {
            finger_id: 1,
            x: btn.x + 10.0,
            y: btn.y + 10.0,
        });
        let f1 = router.finish_frame(0.016, &tf, true, true);
        assert!(f1.show_reference);
        assert_eq!(f1.active_device, InputDeviceKind::Touch);

        // Real mouse move cancels stale touch hold
        router.push_event(PlatformEvent::MouseMove {
            x: tf.viewport.x + 50.0,
            y: tf.viewport.y + 50.0,
            is_touch_synth: false,
        });
        let f2 = router.finish_frame(0.016, &tf, true, true);
        assert!(!f2.show_reference);
        assert_eq!(f2.active_device, InputDeviceKind::Mouse);
    }
}
