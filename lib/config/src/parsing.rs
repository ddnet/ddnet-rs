use command_parser::parser::{CommandArg, CommandArgType};

use crate::traits::ConfigValue;

pub fn find_modifiers(in_str: &str) -> (String, Vec<String>) {
    let mut modifiers = Vec::new();
    let mut cur_modifier = String::new();
    let mut str_without_modifiers = String::new();
    let mut brackets = 0;
    for c in in_str.chars() {
        if c == '[' {
            brackets += 1;
        } else if c == ']' {
            brackets -= 1;

            if brackets == 0 {
                modifiers.push(cur_modifier);
                cur_modifier = String::new();
            }
        } else if brackets == 0 {
            str_without_modifiers.push(c);
        } else {
            cur_modifier.push(c);
        }
    }

    (str_without_modifiers, modifiers)
}

fn struct_name(val: &ConfigValue) -> &str {
    if let ConfigValue::Struct { name, .. } = val {
        name
    } else {
        ""
    }
}

fn parse_conf_value_usage(val: &ConfigValue) -> String {
    match val {
        ConfigValue::Struct { .. } => "".to_string(),
        ConfigValue::Boolean => "boolean [true or false]".to_string(),
        ConfigValue::Int { min, max } => {
            format!("int [{min}..{max}]")
        }
        ConfigValue::Float { min, max } => {
            format!("float [{min:.4},{max:.4}]")
        }
        ConfigValue::String {
            min_length,
            max_length,
        } => {
            format!("string, length range [{min_length}..{max_length}]")
        }
        ConfigValue::Color => {
            "rgb color, supports html syntax (#) & css syntax (rgb())".to_string()
        }
        ConfigValue::StringOfList { allowed_values } => {
            format!("string in [{}]", allowed_values.join(", "))
        }
        ConfigValue::Array { val_ty, .. } => {
            format!(
                "array of [{}] (access/set: [numberic index], remove: `pop`-cmd, \
                insert: `push`-cmd, assign whole array by JSON)",
                struct_name(val_ty)
            )
        }
        ConfigValue::JsonLikeRecord { val_ty } => {
            format!(
                "JSON-like record (access/insert/set: [alphabetic index], \
                rem: `rem`-cmd + [alphabetic index], assign whole record by JSON) \
                {{ \"index\": \"{}\" }}",
                parse_conf_value_usage(val_ty)
            )
        }
    }
}

#[derive(Debug, Clone)]
pub struct AddFeedback {
    pub name: String,
    pub usage: String,
    pub description: String,
    pub args: Vec<CommandArg>,
}

#[derive(Debug, Default)]
pub enum AliasType {
    #[default]
    None,
    /// The alias is simply a rename
    /// e.g. `tee` & `character`.
    RenameOnly,
    /// The alias is a rename but also
    /// has modifiers, e.g. for a list
    /// of `players` an alias could be
    /// `main_player` => `players[0]`
    HasModifiers,
}

pub fn parse_conf_values_as_str_list(
    cur_path: String,
    add: &mut dyn FnMut(AddFeedback, &ConfigValue),
    val: ConfigValue,
    description: String,
    alias_ty: AliasType,
) {
    let usage = parse_conf_value_usage(&val);
    match &val {
        ConfigValue::Struct {
            attributes,
            aliases,
            ..
        } => {
            if !cur_path.is_empty() {
                add(
                    AddFeedback {
                        name: cur_path.clone(),
                        usage,
                        description,
                        args: vec![CommandArg {
                            ty: CommandArgType::JsonObjectLike,
                            user_ty: None,
                        }],
                    },
                    &val,
                );
            }

            for attribute in attributes {
                let mut new_path = cur_path.clone();
                if !cur_path.is_empty() {
                    new_path.push('.');
                }

                let path_without_name = new_path.clone();
                new_path.push_str(&attribute.name);
                parse_conf_values_as_str_list(
                    new_path,
                    add,
                    attribute.val.clone(),
                    attribute.description.clone(),
                    AliasType::None,
                );

                // check if attribute has potential alias
                for (from, to) in aliases {
                    if to
                        .to_lowercase()
                        .starts_with(&attribute.name.to_lowercase())
                    {
                        let (rest, modifiers) = find_modifiers(to.as_str());
                        // quickly recheck if the attribute is really correct
                        if rest.to_lowercase() == attribute.name.to_lowercase() {
                            let mut path = path_without_name.clone();
                            path.push_str(from.as_str());
                            parse_conf_values_as_str_list(
                                path,
                                add,
                                attribute.val.clone(),
                                attribute.description.clone(),
                                if modifiers.is_empty() {
                                    AliasType::RenameOnly
                                } else {
                                    AliasType::HasModifiers
                                },
                            );
                        }
                    }
                }
            }
        }
        ConfigValue::JsonLikeRecord { val_ty } | ConfigValue::Array { val_ty, .. } => {
            let mut new_path = cur_path.clone();

            // and object access/set/etc. of the types
            if matches!(alias_ty, AliasType::None | AliasType::RenameOnly) {
                // push the object itself
                add(
                    AddFeedback {
                        name: cur_path,
                        usage,
                        description,
                        args: vec![CommandArg {
                            ty: if matches!(val, ConfigValue::JsonLikeRecord { .. }) {
                                CommandArgType::JsonObjectLike
                            } else {
                                CommandArgType::JsonArrayLike
                            },
                            user_ty: None,
                        }],
                    },
                    &val,
                );

                if let ConfigValue::JsonLikeRecord { .. } = val {
                    new_path.push_str("$KEY$");
                } else {
                    new_path.push_str("$INDEX$");
                }
            }
            parse_conf_values_as_str_list(
                new_path,
                add,
                *val_ty.clone(),
                "".into(),
                AliasType::None,
            );
        }
        ref conf_val => {
            add(
                AddFeedback {
                    name: cur_path,
                    args: vec![CommandArg {
                        ty: match conf_val {
                            ConfigValue::Float { .. } => CommandArgType::Float,
                            ConfigValue::Int { .. } => CommandArgType::Number,
                            _ => CommandArgType::Text,
                        },
                        user_ty: None,
                    }],
                    description,
                    usage,
                },
                &val,
            );
        }
    }
}
