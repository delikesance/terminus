use crate::geom::Rect;
use crate::theme::ChromeTheme;
use crate::tokens::radius;

pub const FILE_ROW_HEIGHT: f32 = 40.0;
pub const FILE_ROW_RADIUS: f32 = radius::SMALL;
pub const FILE_ROW_PAD_X: f32 = 10.0;
pub const FILE_ROW_GAP: f32 = 12.0;
pub const FILE_ICON_SIZE: f32 = 15.0;
pub const FILE_SIZE_WIDTH: f32 = 70.0;
pub const FILE_DATE_WIDTH: f32 = 80.0;
pub const FILE_RENAME_HEIGHT: f32 = 28.0;
pub const FILE_RENAME_RADIUS: f32 = 6.0;
pub const FILE_DROP_STROKE: f32 = 1.5;
/// Alpha of the drop-target accent fill.
pub const FILE_DROP_FILL_ALPHA: f32 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Folder,
    File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRowState {
    Default,
    Hover,
    Selected,
    DropTarget,
    Renaming,
}

impl FileRowState {
    /// Row fill, `None` when transparent.
    pub fn background(self, theme: &ChromeTheme) -> Option<[f32; 4]> {
        match self {
            FileRowState::Default | FileRowState::Renaming => None,
            FileRowState::Hover => Some(theme.surface),
            FileRowState::Selected => Some(theme.selected),
            FileRowState::DropTarget => {
                let a = theme.accent;
                Some([a[0], a[1], a[2], FILE_DROP_FILL_ALPHA])
            }
        }
    }

    /// Inset accent stroke width, if any.
    pub fn inset_stroke(self) -> Option<f32> {
        (self == FileRowState::DropTarget).then_some(FILE_DROP_STROKE)
    }

    pub fn shows_rename_field(self) -> bool {
        self == FileRowState::Renaming
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileRowLayout {
    pub rect: Rect,
    pub icon: Rect,
    pub name: Rect,
    /// The inline rename field (28 px, centered over the name column).
    pub rename_field: Rect,
    pub size: Rect,
    pub date: Rect,
}

pub fn file_row_layout(rect: Rect) -> FileRowLayout {
    let mid = rect.y + rect.height / 2.0;
    let date = Rect::new(
        rect.right() - FILE_ROW_PAD_X - FILE_DATE_WIDTH,
        rect.y,
        FILE_DATE_WIDTH,
        rect.height,
    );
    let size = Rect::new(
        date.x - FILE_ROW_GAP - FILE_SIZE_WIDTH,
        rect.y,
        FILE_SIZE_WIDTH,
        rect.height,
    );
    let icon = Rect::new(
        rect.x + FILE_ROW_PAD_X,
        mid - FILE_ICON_SIZE / 2.0,
        FILE_ICON_SIZE,
        FILE_ICON_SIZE,
    );
    let name_x = icon.right() + FILE_ROW_GAP;
    let name = Rect::new(
        name_x,
        rect.y,
        (size.x - FILE_ROW_GAP - name_x).max(0.0),
        rect.height,
    );
    let rename_field = Rect::new(
        name.x,
        mid - FILE_RENAME_HEIGHT / 2.0,
        name.width,
        FILE_RENAME_HEIGHT,
    );
    FileRowLayout {
        rect,
        icon,
        name,
        rename_field,
        size,
        date,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileColumn {
    Icon,
    Name,
    Size,
    Date,
    /// The row's padding / gaps.
    Row,
}

pub fn file_row_hit(rect: Rect, x: f32, y: f32) -> Option<FileColumn> {
    if !rect.contains(x, y) {
        return None;
    }
    let l = file_row_layout(rect);
    let in_x = |r: &Rect| x >= r.x && x < r.right();
    Some(if in_x(&l.icon) {
        FileColumn::Icon
    } else if in_x(&l.name) {
        FileColumn::Name
    } else if in_x(&l.size) {
        FileColumn::Size
    } else if in_x(&l.date) {
        FileColumn::Date
    } else {
        FileColumn::Row
    })
}
