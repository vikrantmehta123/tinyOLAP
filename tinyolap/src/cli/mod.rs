//! Interactive REPL shell for tinyOLAP.
//!
//! Wraps rustyline so we get readline-style editing (arrow keys, history).
//! Used in main.rs as the entrypoint of the program for now.
//! TODO: We would want to have this as a separate command line utility that can
//! connect to the database rather than an entry point.

use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;


pub struct Repl {
    editor: DefaultEditor,
}

impl Repl {
    pub fn new() -> rustyline::Result<Self> {
        let editor = DefaultEditor::new()?;
        Ok(Self { editor })
    }

    pub fn next_line(&mut self, prompt: &str) -> Option<String> {
        match self.editor.readline(prompt) {
            Ok(line) => {
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    let _ = self.editor.add_history_entry(&trimmed);
                }
                Some(trimmed)
            }
            Err(ReadlineError::Interrupted) => None,
            Err(ReadlineError::Eof) => None,
            Err(e) => {
                eprintln!("readline error: {e}");
                None
            }
        }
    }
}