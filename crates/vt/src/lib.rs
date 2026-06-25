//! Terminal emulation core: parser handling, grid, screens, cursor, modes.

pub mod cell;
pub mod charset;
pub mod color;
pub mod cursor;
pub mod grid;
pub mod hyperlink;
pub mod kitty;
pub mod mode;
pub mod screen;
pub mod search;
pub mod selection;
pub mod sgr;
pub mod sixel;
pub mod term;
pub mod url;

pub use cell::{Cell, CellFlags};
pub use charset::{Charset, Charsets};
pub use color::{Color, NamedColor};
pub use cursor::{Cursor, CursorStyle, SavedCursor};
pub use grid::damage::{Damage, DamageTracker};
pub use grid::row::Row;
pub use grid::scrollback::{Scrollback, DEFAULT_SCROLLBACK};
pub use grid::Grid;
pub use hyperlink::{Hyperlink, HyperlinkId, Hyperlinks};
pub use kitty::KittyKeyboard;
pub use mode::{Modes, MouseMode};
pub use screen::Screen;
pub use search::Match;
pub use selection::{Point, Selection, SelectionMode};
pub use sixel::Image;
pub use term::{Clipboard, Notification, ReportColors, Terminal};
