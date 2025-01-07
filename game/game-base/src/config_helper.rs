use std::ops::Range;

use command_parser::parser::{self, Syn};

pub fn handle_config_variable_cmd(
    cmd: &parser::Command,
    config: &mut dyn config::traits::ConfigInterface,
) -> anyhow::Result<String> {
    fn syn_vec_to_config_val(args: &[(Syn, Range<usize>)]) -> Option<String> {
        args.first().map(|(arg, _)| match arg {
            parser::Syn::Command(cmd) => cmd.cmd_text.clone(),
            parser::Syn::Commands(cmds) => cmds
                .first()
                .map(|cmd| cmd.cmd_text.clone())
                .unwrap_or_default(),
            parser::Syn::Text(text) => text.clone(),
            parser::Syn::Number(num) => num.clone(),
            parser::Syn::Float(num) => num.clone(),
            parser::Syn::JsonObjectLike(obj) => obj.clone(),
            parser::Syn::JsonArrayLike(obj) => obj.clone(),
        })
    }
    Ok(config.try_set_from_str(
        cmd.cmd_text.clone(),
        None,
        syn_vec_to_config_val(&cmd.args),
        None,
        config::traits::ConfigFromStrOperation::Set,
    )?)
}
