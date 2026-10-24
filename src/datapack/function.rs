use std::collections::BTreeSet;

use super::command::Command;

#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Line {
    Command(Command),
    /// Rendered as `# text`; multi-line text becomes several comment lines.
    Comment(String),
    Blank,
}

/// The body of one `.mcfunction` file.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Function {
    lines: Vec<Line>,
}

impl Function {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, cmd: Command) -> &mut Self {
        self.lines.push(Line::Command(cmd));
        self
    }

    pub fn comment(&mut self, text: impl Into<String>) -> &mut Self {
        self.lines.push(Line::Comment(text.into()));
        self
    }

    pub fn blank(&mut self) -> &mut Self {
        self.lines.push(Line::Blank);
        self
    }

    pub fn extend(&mut self, cmds: impl IntoIterator<Item = Command>) -> &mut Self {
        self.lines.extend(cmds.into_iter().map(Line::Command));
        self
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    pub fn commands(&self) -> impl Iterator<Item = &Command> {
        self.lines.iter().filter_map(|l| match l {
            Line::Command(c) => Some(c),
            _ => None,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.commands().next().is_none()
    }

    /// Macro variables used by this function's macro lines. A function with any
    /// is a "macro function" and must be called with arguments.
    pub fn macro_vars(&self) -> BTreeSet<String> {
        let mut vars = Vec::new();
        for c in self.commands() {
            c.macro_vars(&mut vars);
        }
        vars.into_iter().collect()
    }

    pub fn is_macro_function(&self) -> bool {
        self.commands().any(Command::is_macro)
    }

    /// The file contents, with a trailing newline. Does not validate; see
    /// [`DataPack::validate`](super::DataPack::validate).
    pub fn render(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            match line {
                Line::Command(c) => out.push_str(&c.render_line()),
                Line::Comment(text) => {
                    for (i, part) in text.lines().enumerate() {
                        if i > 0 {
                            out.push('\n');
                        }
                        out.push_str("# ");
                        out.push_str(part);
                    }
                    if text.is_empty() {
                        out.push('#');
                    }
                }
                Line::Blank => {}
            }
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datapack::command::MacroCommand;

    #[test]
    fn renders_lines() {
        let mut f = Function::new();
        f.comment("setup\nsecond line").push(Command::say("hi")).blank().push(Command::Macro(MacroCommand::new().text("say ").var("x")));
        assert_eq!(f.render(), "# setup\n# second line\nsay hi\n\n$say $(x)\n");
        assert!(f.is_macro_function());
        assert_eq!(f.macro_vars().into_iter().collect::<Vec<_>>(), ["x"]);
    }
}
