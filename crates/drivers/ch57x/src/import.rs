//! ch57x-keyboard-tool YAML configs → standard keymaps.
//!
//! Each macro becomes a `raw` action holding its expression, so nothing is
//! lost in translation. Orientation is applied on the way in: the mapping
//! always lists slots in the keypad's own order.

use std::collections::BTreeMap;

use anyhow::{Context as _, Result};
use itertools::Itertools as _;

use ch57x::config::{Config, KeyboardModel};
use ch57x::keyboard::{Macro, MouseAction, MouseEvent};
use okf_core::model::{Action, ActionKind, KNOB_ACTIONS, Layer, Mapping};

use crate::{DRIVER_ID, model_id};

/// A macro in the expression syntax the parser reads back. `Macro`'s own
/// `Display` writes clicks as `left+right`, which would read as arrow keys.
pub fn expression(m: &Macro) -> String {
    match m {
        Macro::Mouse(MouseEvent(MouseAction::Click(buttons), modifier)) => {
            let prefix = modifier.map(|m| format!("{m}-")).unwrap_or_default();
            format!("{prefix}click({})", buttons.iter().format("+")).to_lowercase()
        }
        other => other.to_string().to_lowercase(),
    }
}

pub fn from_yaml(text: &str) -> Result<Mapping> {
    let config: Config = serde_yaml::from_str(text).context("read YAML config")?;
    let model = config.model.unwrap_or(KeyboardModel::Ch57x_1);
    let (rows, columns, knobs) = (config.rows, config.columns, config.knobs);
    let raw = |m: &Macro| Action {
        label: None,
        kind: ActionKind::Raw {
            expr: expression(m),
        },
    };

    let layers = config
        .render()?
        .into_iter()
        .map(|flat| {
            let mut slots = BTreeMap::new();
            for (i, m) in flat.buttons.iter().enumerate() {
                if let Some(m) = m {
                    slots.insert(format!("b{i}"), raw(m));
                }
            }
            for (k, knob) in flat.knobs.iter().enumerate() {
                for (action, m) in KNOB_ACTIONS.iter().zip([&knob.ccw, &knob.press, &knob.cw]) {
                    if let Some(m) = m {
                        slots.insert(format!("k{k}.{action}"), raw(m));
                    }
                }
            }
            Layer {
                name: None,
                led: None,
                slots,
            }
        })
        .collect();

    Ok(Mapping {
        driver: DRIVER_ID.into(),
        model: model_id(model).into(),
        rows,
        columns,
        knobs,
        layers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_the_example_config() -> Result<()> {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../engines/ch57x/example-mapping.yaml"
        ))?;
        let m = from_yaml(&text)?;
        assert_eq!(m.model, "ch57x-2");
        assert_eq!((m.rows, m.columns, m.knobs), (3, 4, 2));
        // Every expression must read back as the same macro.
        let errors = okf_core::driver::slot_errors(&crate::Ch57xDriver, &m)?;
        let unreadable: Vec<_> = errors
            .iter()
            .filter(|(_, e)| e.contains("can't read"))
            .collect();
        assert!(unreadable.is_empty(), "{unreadable:?}");
        Ok(())
    }

    #[test]
    fn click_expressions_round_trip() {
        for src in ["click(left+right)", "ctrl-click(middle)", "rclick"] {
            let m: Macro = src.parse().unwrap();
            assert_eq!(expression(&m).parse::<Macro>().unwrap(), m, "{src}");
        }
    }
}
