//! Text components, rendered as SNBT (docs/mc/04-data-formats.md section 2).
//! Since 1.21.5 these are NBT, not JSON strings.

use super::args::{DataTarget, NamedColor, ScoreHolder, Selector};
use super::command::Command;
use super::error::{Check, Result, bail};
use super::names::{Objective, ResourceLocation};
use super::nbt::{Compound, Nbt, NbtPath};

#[derive(Clone, Debug, PartialEq)]
pub enum Color {
    Named(NamedColor),
    /// `#RRGGBB`
    Hex(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    Plain(String),
    Translate { key: String, fallback: Option<String>, with: Vec<Text> },
    /// Resolved by the server when sent; `name` may be a selector, a holder name or `*`.
    Score { name: ScoreHolder, objective: Objective },
    Selector { selector: Selector, separator: Option<Box<Text>> },
    Keybind(String),
    Nbt { path: NbtPath, source: DataTarget, interpret: bool },
}

/// Click actions. `RunCommand`/`SuggestCommand` take a typed [`Command`], rendered without
/// a leading slash (26.3 does not need one).
#[derive(Clone, Debug, PartialEq)]
pub enum ClickEvent {
    RunCommand(Box<Command>),
    SuggestCommand(Box<Command>),
    OpenUrl(String),
    CopyToClipboard(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum HoverEvent {
    ShowText(Box<Text>),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Style {
    pub color: Option<Color>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underlined: Option<bool>,
    pub strikethrough: Option<bool>,
    pub obfuscated: Option<bool>,
    pub font: Option<ResourceLocation>,
    pub insertion: Option<String>,
    pub click: Option<ClickEvent>,
    pub hover: Option<HoverEvent>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub content: Content,
    pub style: Style,
    pub extra: Vec<Text>,
}

impl Text {
    pub fn new(content: Content) -> Self {
        Text { content, style: Style::default(), extra: Vec::new() }
    }

    pub fn plain(s: impl Into<String>) -> Self {
        Text::new(Content::Plain(s.into()))
    }

    pub fn score(name: impl Into<ScoreHolder>, objective: Objective) -> Self {
        Text::new(Content::Score { name: name.into(), objective })
    }

    pub fn selector(selector: Selector) -> Self {
        Text::new(Content::Selector { selector, separator: None })
    }

    pub fn translate(key: impl Into<String>) -> Self {
        Text::new(Content::Translate { key: key.into(), fallback: None, with: Vec::new() })
    }

    pub fn color(mut self, c: NamedColor) -> Self {
        self.style.color = Some(Color::Named(c));
        self
    }

    pub fn hex_color(mut self, rgb: u32) -> Self {
        self.style.color = Some(Color::Hex(rgb));
        self
    }

    pub fn bold(mut self, on: bool) -> Self {
        self.style.bold = Some(on);
        self
    }

    pub fn italic(mut self, on: bool) -> Self {
        self.style.italic = Some(on);
        self
    }

    pub fn on_click(mut self, e: ClickEvent) -> Self {
        self.style.click = Some(e);
        self
    }

    pub fn on_hover(mut self, e: HoverEvent) -> Self {
        self.style.hover = Some(e);
        self
    }

    pub fn then(mut self, next: Text) -> Self {
        self.extra.push(next);
        self
    }

    /// The component as an NBT value, ready to embed in SNBT output.
    pub fn to_nbt(&self) -> Nbt {
        let is_bare = self.style == Style::default() && self.extra.is_empty();
        if let (true, Content::Plain(s)) = (is_bare, &self.content) {
            return Nbt::String(s.clone());
        }
        let mut c = Compound::new();
        match &self.content {
            Content::Plain(s) => c.insert("text", s.as_str()),
            Content::Translate { key, fallback, with } => {
                c.insert("translate", key.as_str());
                if let Some(fb) = fallback {
                    c.insert("fallback", fb.as_str());
                }
                if !with.is_empty() {
                    c.insert("with", Nbt::List(with.iter().map(Text::to_nbt).collect()));
                }
            }
            Content::Score { name, objective } => c.insert(
                "score",
                Compound::new().with("name", name.to_string()).with("objective", objective.to_string()),
            ),
            Content::Selector { selector, separator } => {
                c.insert("selector", selector.to_string());
                if let Some(sep) = separator {
                    c.insert("separator", sep.to_nbt());
                }
            }
            Content::Keybind(k) => c.insert("keybind", k.as_str()),
            Content::Nbt { path, source, interpret } => {
                c.insert("nbt", path.to_string());
                match source {
                    DataTarget::Block(p) => c.insert("block", p.to_string()),
                    DataTarget::Entity(s) => c.insert("entity", s.to_string()),
                    DataTarget::Storage(id) => c.insert("storage", id.to_string()),
                }
                if *interpret {
                    c.insert("interpret", true);
                }
            }
        }
        let s = &self.style;
        match &s.color {
            Some(Color::Named(n)) => c.insert("color", n.as_str()),
            Some(Color::Hex(rgb)) => c.insert("color", format!("#{:06x}", rgb & 0xFF_FFFF)),
            None => {}
        }
        for (key, v) in [
            ("bold", s.bold),
            ("italic", s.italic),
            ("underlined", s.underlined),
            ("strikethrough", s.strikethrough),
            ("obfuscated", s.obfuscated),
        ] {
            if let Some(v) = v {
                c.insert(key, v);
            }
        }
        if let Some(font) = &s.font {
            c.insert("font", font.to_string());
        }
        if let Some(ins) = &s.insertion {
            c.insert("insertion", ins.as_str());
        }
        if let Some(click) = &s.click {
            let e = match click {
                ClickEvent::RunCommand(cmd) => click_obj("run_command", "command", cmd.to_string()),
                ClickEvent::SuggestCommand(cmd) => click_obj("suggest_command", "command", cmd.to_string()),
                ClickEvent::OpenUrl(u) => click_obj("open_url", "url", u.clone()),
                ClickEvent::CopyToClipboard(v) => click_obj("copy_to_clipboard", "value", v.clone()),
            };
            c.insert("click_event", e);
        }
        if let Some(HoverEvent::ShowText(t)) = &s.hover {
            c.insert("hover_event", Compound::new().with("action", "show_text").with("value", t.to_nbt()));
        }
        if !self.extra.is_empty() {
            c.insert("extra", Nbt::List(self.extra.iter().map(Text::to_nbt).collect()));
        }
        Nbt::Compound(c)
    }
}

fn click_obj(action: &str, field: &str, value: String) -> Compound {
    Compound::new().with("action", action).with(field, value)
}

impl From<&str> for Text {
    fn from(s: &str) -> Text {
        Text::plain(s)
    }
}

impl std::fmt::Display for Text {
    /// The SNBT form, as written after `tellraw <targets>`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_nbt())
    }
}

impl Check for Text {
    fn check(&self) -> Result<()> {
        match &self.content {
            Content::Score { name, .. } => name.check()?,
            Content::Selector { selector, separator } => {
                selector.check()?;
                separator.check()?;
            }
            Content::Translate { with, .. } => with.check()?,
            Content::Nbt { path, source, .. } => {
                path.check()?;
                source.check()?;
                if path.is_empty() {
                    bail!("nbt text component needs a path");
                }
            }
            Content::Plain(_) | Content::Keybind(_) => {}
        }
        if let Some(ClickEvent::RunCommand(c) | ClickEvent::SuggestCommand(c)) = &self.style.click {
            c.check()?;
            if c.is_macro() {
                bail!("a click event cannot run a macro command");
            }
        }
        if let Some(HoverEvent::ShowText(t)) = &self.style.hover {
            t.check()?;
        }
        if let Some(ClickEvent::OpenUrl(u)) = &self.style.click
            && !(u.starts_with("http://") || u.starts_with("https://")) {
                bail!("open_url must be an http(s) URL, got {u:?}");
            }
        self.extra.check()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datapack::names::{rl, word};

    #[test]
    fn bare_string_shorthand() {
        assert_eq!(Text::plain("hi").to_string(), r#""hi""#);
    }

    #[test]
    fn styled_and_extra() {
        let t = Text::plain("Score: ")
            .color(NamedColor::Gold)
            .bold(true)
            .then(Text::score(Selector::s(), word("pts")).color(NamedColor::Red));
        assert_eq!(
            t.to_string(),
            r#"{text:"Score: ",color:"gold",bold:1b,extra:[{score:{name:"@s",objective:"pts"},color:"red"}]}"#
        );
    }

    #[test]
    fn nbt_content_and_hex() {
        let t = Text::new(Content::Nbt {
            path: NbtPath::new().key("a"),
            source: DataTarget::Storage(rl("ns:x")),
            interpret: false,
        })
        .hex_color(0x00ff7f);
        assert_eq!(t.to_string(), r##"{nbt:"a",storage:"ns:x",color:"#00ff7f"}"##);
    }
}
