//! 窗口几何计算（纯函数，全部用物理像素）：吸附、四角停靠、恢复位置、聊天窗口摆放。
//! 与 Tauri 无关，便于单元测试。

use crate::settings::Corner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn center(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
    pub fn contains_point(&self, (px, py): (i32, i32)) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }
    /// 与另一个矩形重叠的面积
    pub fn intersection_area(&self, o: &Rect) -> i64 {
        let w = (self.right().min(o.right()) - self.x.max(o.x)).max(0) as i64;
        let h = (self.bottom().min(o.bottom()) - self.y.max(o.y)).max(0) as i64;
        w * h
    }
}

/// 把窗口完全放进工作区（窗口比工作区大时贴左上）
pub fn clamp_into(win: Rect, work: Rect) -> (i32, i32) {
    let x = win.x.min(work.right() - win.w).max(work.x);
    let y = win.y.min(work.bottom() - win.h).max(work.y);
    (x, y)
}

/// 松手后吸附：离工作区边缘不到 `threshold` 像素就贴上去，并保证完全可见。
pub fn snap_to_edges(win: Rect, work: Rect, threshold: i32) -> (i32, i32) {
    let (mut x, mut y) = clamp_into(win, work);
    if (x - work.x).abs() <= threshold {
        x = work.x;
    } else if (work.right() - (x + win.w)).abs() <= threshold {
        x = work.right() - win.w;
    }
    if (y - work.y).abs() <= threshold {
        y = work.y;
    } else if (work.bottom() - (y + win.h)).abs() <= threshold {
        y = work.bottom() - win.h;
    }
    (x, y)
}

/// 停靠到工作区某个角落（留 `margin` 像素边距）
pub fn corner_position(corner: Corner, (w, h): (i32, i32), work: Rect, margin: i32) -> (i32, i32) {
    let left = work.x + margin;
    let right = work.right() - w - margin;
    let top = work.y + margin;
    let bottom = work.bottom() - h - margin;
    match corner {
        Corner::TopLeft => (left, top),
        Corner::TopRight => (right, top),
        Corner::BottomLeft => (left, bottom),
        Corner::BottomRight => (right, bottom),
    }
}

/// 窗口所在的显示器：重叠面积最大的那个；都不重叠时返回 None
pub fn pick_monitor(win: Rect, works: &[Rect]) -> Option<usize> {
    works
        .iter()
        .enumerate()
        .map(|(i, m)| (i, m.intersection_area(&win)))
        .filter(|(_, a)| *a > 0)
        .max_by_key(|(_, a)| *a)
        .map(|(i, _)| i)
}

/// 恢复上次位置：窗口还和某个显示器的工作区有重叠就留在那块屏幕上（并夹紧到完全可见），
/// 否则（比如那块显示器被拔掉了）回到主屏幕的默认角落。
pub fn restore_position(
    saved: Option<(i32, i32)>,
    size: (i32, i32),
    works: &[Rect],
    primary_work: Rect,
    corner: Corner,
    margin: i32,
) -> (i32, i32) {
    if let Some((x, y)) = saved {
        let win = Rect::new(x, y, size.0, size.1);
        if let Some(i) = pick_monitor(win, works) {
            return clamp_into(win, works[i]);
        }
    }
    corner_position(corner, size, primary_work, margin)
}

/// 聊天窗口相对桌宠的方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// 聊天窗口在桌宠左边（气泡尾巴朝右）
    Left,
    /// 聊天窗口在桌宠右边（气泡尾巴朝左）
    Right,
}

/// 聊天窗口位置：桌宠在屏幕右半边就放左边，否则放右边；
/// 桌宠在下半屏时底边对齐，上半屏时顶边对齐；最后夹紧到工作区内。
pub fn chat_position(pet: Rect, (cw, ch): (i32, i32), work: Rect, gap: i32) -> (i32, i32, Side) {
    let (pcx, pcy) = pet.center();
    let (wcx, wcy) = work.center();
    let mut side = if pcx >= wcx { Side::Left } else { Side::Right };
    let mut x = match side {
        Side::Left => pet.x - gap - cw,
        Side::Right => pet.right() + gap,
    };
    // 放不下就换另一边
    if x < work.x || x + cw > work.right() {
        let (alt_side, alt_x) = match side {
            Side::Left => (Side::Right, pet.right() + gap),
            Side::Right => (Side::Left, pet.x - gap - cw),
        };
        if alt_x >= work.x && alt_x + cw <= work.right() {
            side = alt_side;
            x = alt_x;
        }
    }
    let y = if pcy >= wcy { pet.bottom() - ch } else { pet.y };
    let (x, y) = clamp_into(Rect::new(x, y, cw, ch), work);
    (x, y, side)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect::new(0, 0, 1920, 1040); // 1080p 减去任务栏

    #[test]
    fn snap_near_right_bottom_edges() {
        let win = Rect::new(1920 - 160 - 10, 1040 - 180 - 5, 160, 180);
        assert_eq!(snap_to_edges(win, WORK, 24), (1760, 860));
    }

    #[test]
    fn no_snap_in_middle() {
        let win = Rect::new(800, 400, 160, 180);
        assert_eq!(snap_to_edges(win, WORK, 24), (800, 400));
    }

    #[test]
    fn snap_pulls_offscreen_window_back() {
        let win = Rect::new(-50, 2000, 160, 180);
        assert_eq!(snap_to_edges(win, WORK, 24), (0, 860));
    }

    #[test]
    fn snap_respects_monitor_offset() {
        // 副屏在主屏左边，坐标为负
        let work = Rect::new(-1280, 0, 1280, 984);
        let win = Rect::new(-1270, 100, 160, 180);
        assert_eq!(snap_to_edges(win, work, 24), (-1280, 100));
    }

    #[test]
    fn corners() {
        let s = (160, 180);
        assert_eq!(
            corner_position(Corner::BottomRight, s, WORK, 8),
            (1752, 852)
        );
        assert_eq!(corner_position(Corner::TopLeft, s, WORK, 8), (8, 8));
        assert_eq!(corner_position(Corner::TopRight, s, WORK, 8), (1752, 8));
        assert_eq!(corner_position(Corner::BottomLeft, s, WORK, 8), (8, 852));
    }

    #[test]
    fn restore_on_existing_monitor() {
        let works = [WORK, Rect::new(1920, 0, 2560, 1400)];
        let p = restore_position(
            Some((3000, 500)),
            (160, 180),
            &works,
            WORK,
            Corner::BottomRight,
            8,
        );
        assert_eq!(p, (3000, 500));
    }

    #[test]
    fn restore_falls_back_when_monitor_unplugged() {
        // 上次在副屏（x=3000），现在只剩主屏
        let p = restore_position(
            Some((3000, 500)),
            (160, 180),
            &[WORK],
            WORK,
            Corner::BottomRight,
            8,
        );
        assert_eq!(p, (1752, 852));
    }

    #[test]
    fn restore_clamps_partially_visible() {
        let p = restore_position(
            Some((1850, 100)),
            (160, 180),
            &[WORK],
            WORK,
            Corner::BottomRight,
            8,
        );
        assert_eq!(p, (1760, 100));
    }

    #[test]
    fn restore_default_when_none() {
        let p = restore_position(None, (160, 180), &[WORK], WORK, Corner::TopLeft, 8);
        assert_eq!(p, (8, 8));
    }

    #[test]
    fn pick_monitor_by_largest_overlap() {
        let works = [WORK, Rect::new(1920, 0, 1920, 1040)];
        assert_eq!(pick_monitor(Rect::new(1900, 10, 160, 180), &works), Some(1));
        assert_eq!(pick_monitor(Rect::new(1800, 10, 160, 180), &works), Some(0));
        assert_eq!(pick_monitor(Rect::new(9000, 10, 160, 180), &works), None);
    }

    #[test]
    fn chat_goes_left_of_pet_in_bottom_right() {
        let pet = Rect::new(1752, 852, 160, 180);
        let (x, y, side) = chat_position(pet, (340, 220), WORK, 4);
        assert_eq!(side, Side::Left);
        assert_eq!(x, 1752 - 4 - 340);
        assert_eq!(y + 220, pet.bottom()); // 底边对齐
    }

    #[test]
    fn chat_goes_right_of_pet_in_top_left() {
        let pet = Rect::new(8, 8, 160, 180);
        let (x, y, side) = chat_position(pet, (340, 220), WORK, 4);
        assert_eq!(side, Side::Right);
        assert_eq!((x, y), (172, 8));
    }

    #[test]
    fn chat_switches_side_when_no_room() {
        // 桌宠在右半屏但屏幕很窄，左边放不下
        let work = Rect::new(0, 0, 500, 800);
        let pet = Rect::new(260, 600, 160, 180);
        let (_, _, side) = chat_position(pet, (340, 220), work, 4);
        // 两边都放不下时保留原方向并夹紧到屏幕内
        assert_eq!(side, Side::Left);
        let pet = Rect::new(400, 600, 100, 180);
        let (x, _, side) = chat_position(pet, (340, 220), work, 4);
        assert_eq!(side, Side::Left);
        assert_eq!(x, 56);
    }

    #[test]
    fn chat_tall_panel_is_clamped_into_work_area() {
        let pet = Rect::new(1752, 100, 160, 180);
        let (_, y, _) = chat_position(pet, (360, 520), WORK, 4);
        assert!(y >= 0 && y + 520 <= 1040);
    }
}
