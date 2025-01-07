use command_parser::parser::{CommandParseResult, CommandType, CommandsTyped};
use egui::{Color32, FontId, RichText};

/// console input err
pub fn render(ui: &mut egui::Ui, msg: &str, cmds: &CommandsTyped) {
    fn find_err(res: &CommandParseResult) -> &CommandParseResult {
        match res {
            CommandParseResult::InvalidCommandIdent { .. }
            | CommandParseResult::InvalidArg { .. }
            | CommandParseResult::InvalidQuoteParsing(_)
            | CommandParseResult::Other { .. } => res,
            CommandParseResult::InvalidCommandArg { err, .. }
            | CommandParseResult::InvalidCommandsArg { err, .. } => err,
        }
    }

    let err = cmds.iter().rev().find_map(|cmd| {
        if let CommandType::Partial(cmd) = cmd {
            Some(find_err(cmd))
        } else {
            None
        }
    });
    if let Some(err_range) = err.as_ref().map(|err| err.range()) {
        let mut start_chars = 0;
        let mut err_chars = 0;
        msg.char_indices().for_each(|(byte_index, _)| {
            if byte_index < err_range.start {
                start_chars += 1;
            } else if byte_index >= err_range.start && byte_index <= err_range.end {
                err_chars += 1;
            }
        });

        ui.horizontal_top(|ui| {
            ui.add_space(9.0);
            // two whitespaces for `>` console prefix
            let mut tilde = " ".to_string();
            for _ in 0..start_chars {
                tilde.push(' ');
            }
            for _ in 0..err_chars {
                tilde.push('~');
            }
            ui.label(
                RichText::new(tilde)
                    .font(FontId::monospace(12.0))
                    .color(Color32::RED),
            );
        });
    }
    if let Some(err) = err {
        ui.label(err.to_string());
    }
}
