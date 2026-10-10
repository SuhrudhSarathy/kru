//! `task add` and `task mark` get one flag per editable task field, built
//! from [`Task::fields`], so a new field gets its flag without changes here.

use clap::{Arg, ArgMatches, Command};
use kru::task::{Field, Task, Value};

/// A field to set and the user's input for it, e.g. `("due", "fri")`.
pub type FieldInput = (&'static str, String);

/// Adds a flag per editable field: `--<key>`, plus the key's first letter as
/// a short flag if it is still free. Long flags can be shortened while
/// unambiguous, so `--des` works for `--description`.
pub fn add_flags(mut command: Command) -> Command {
    let mut taken: Vec<char> = command.get_arguments().filter_map(Arg::get_short).collect();
    taken.push('h');
    for field in editable() {
        let mut arg = Arg::new(field.key).long(field.key).help(help(&field));
        if let Some(short) = field.key.chars().next().filter(|c| !taken.contains(c)) {
            arg = arg.short(short);
            taken.push(short);
        }
        command = command.arg(arg);
    }
    command.infer_long_args(true)
}

/// The field flags given in `matches`, in field order.
pub fn given(matches: &ArgMatches) -> Vec<FieldInput> {
    editable()
        .into_iter()
        .filter_map(|field| {
            let input = matches.try_get_one::<String>(field.key).ok().flatten()?;
            Some((field.key, input.clone()))
        })
        .collect()
}

/// Every field flag, e.g. `--description, --priority, --status, --due`.
pub fn flag_list() -> String {
    let flags: Vec<String> = editable().iter().map(|f| format!("--{}", f.key)).collect();
    flags.join(", ")
}

fn editable() -> Vec<Field> {
    Task::new("")
        .fields()
        .into_iter()
        .filter(Field::is_editable)
        .collect()
}

/// Help text from the field's kind.
fn help(field: &Field) -> String {
    match &field.value {
        Value::Choice { options, .. } => format!("{}: {}", field.label, options.join(", ")),
        Value::Time(_) => format!(
            "{}: 2026-10-12, \"2026-10-12 17:00\", today, tomorrow, fri, 3d, 2h (none clears it)",
            field.label
        ),
        Value::Text(_) => field.label.to_string(),
    }
}
