use std::io::Write;
use std::process::{Command, Stdio};

/// Copy text to the system clipboard. Returns true on success.
pub fn copy(text: &str) -> bool {
    // Try clipboard commands in order of preference
    let commands = clipboard_commands();

    for (cmd, args) in &commands {
        if let Ok(mut child) = Command::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            && let Some(mut stdin) = child.stdin.take()
            && stdin.write_all(text.as_bytes()).is_ok()
        {
            drop(stdin);
            if let Ok(status) = child.wait()
                && status.success()
            {
                return true;
            }
        }
    }

    false
}

fn clipboard_commands() -> Vec<(&'static str, Vec<&'static str>)> {
    if cfg!(target_os = "macos") {
        vec![("pbcopy", vec![])]
    } else if cfg!(target_os = "windows") {
        vec![("clip", vec![])]
    } else {
        // Linux: try wayland first, then X11
        let mut cmds = Vec::new();
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            cmds.push(("wl-copy", vec![]));
        }
        cmds.push(("xclip", vec!["-selection", "clipboard"]));
        cmds.push(("xsel", vec!["--clipboard", "--input"]));
        cmds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_commands_returns_non_empty() {
        let cmds = clipboard_commands();
        assert!(
            !cmds.is_empty(),
            "Should have at least one clipboard command"
        );
    }

    #[test]
    fn test_copy_does_not_panic_on_any_input() {
        // Should not panic even if no clipboard tool is available
        let _ = copy("test text");
        let _ = copy("");
        let _ = copy("multi\nline\ntext");
    }
}
