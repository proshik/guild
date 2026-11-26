//! Per-session model choices. Never edits a user's provider configuration or permissions.
use super::HarnessKind;
use serde::{Deserialize, Serialize};

/// Optional role choices retained with the actual session for same-harness recovery.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchSelection {
    /// Explicit model, with provider qualification where the harness requires it.
    pub model: Option<String>,
    /// Explicit effort/variant, never a permission or approval setting.
    pub effort: Option<String>,
}
impl LaunchSelection {
    /// Validate literal selections without evaluating shell text or contacting a provider.
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [("model", &self.model), ("effort", &self.effort)] {
            Self::validate_option(name, value.as_deref())?;
        }
        Ok(())
    }
    /// One literal rule for frozen role definitions and restored session selections.
    pub(crate) fn validate_option(name: &str, value: Option<&str>) -> Result<(), String> {
        if value.is_some_and(|s| {
            s.trim().is_empty()
                || s.len() > 512
                || s.chars().any(char::is_control)
                || s.starts_with('-')
        }) {
            return Err(format!(
                "harness.{name}: expected bounded nonempty literal selection"
            ));
        }
        Ok(())
    }
    /// Actual CLI options. OpenCode v2 uses its existing per-launch config channel.
    pub fn args(&self, harness: HarnessKind) -> Result<Vec<String>, String> {
        self.validate()?;
        let mut args = Vec::new();
        if harness == HarnessKind::OpenCode {
            return Ok(args);
        }
        if let Some(model) = &self.model {
            args.extend(["--model".into(), model.clone()]);
        }
        if let Some(effort) = &self.effort {
            match harness {
                HarnessKind::Claude => args.extend(["--effort".into(), effort.clone()]),
                HarnessKind::Codex => args.extend([
                    "-c".into(),
                    format!(
                        "model_reasoning_effort={}",
                        serde_json::to_string(effort).map_err(|e| e.to_string())?
                    ),
                ]),
                HarnessKind::OpenCode => unreachable!("OpenCode returned above"),
            }
        }
        Ok(args)
    }
    /// Compose only explicit selection fields into inherited strict JSON.
    /// None means leave the inherited bytes entirely untouched, including JSONC.
    pub fn opencode_inline(&self, inherited: Option<&str>) -> Result<Option<String>, String> {
        self.validate()?;
        if self.model.is_none() && self.effort.is_none() {
            return Ok(None);
        }
        let mut config:serde_json::Value=match inherited {
            Some(text)=>serde_json::from_str(text).map_err(|_|"harness.opencode: inherited inline config is not strict JSON; explicit selection was not applied")?,
            None=>serde_json::json!({}),
        };
        let object = config.as_object_mut().ok_or(
            "harness.opencode: inherited inline config is not an object; preserved unchanged",
        )?;
        if self
            .effort
            .as_ref()
            .is_some_and(|effort| effort.contains('#'))
        {
            return Err("harness.opencode: variant cannot contain #".into());
        }
        let mut selected=self.model.as_ref().map(|model|serde_json::Value::String(model.clone()))
            .or_else(||object.get("model").cloned())
            .ok_or("harness.opencode: an explicit provider/model is required to pin effort; inherited filesystem selection is unknown")?;
        match &mut selected {
            serde_json::Value::String(model)=>{
                let (base,variant)=model.split_once('#').map_or((model.as_str(),None),|(base,variant)|(base,Some(variant)));
                if variant.is_some_and(|value|value.is_empty() || value.contains('#')) { return Err("harness.opencode: invalid model variant".into()); }
                let Some((provider,name))=base.split_once('/') else {return Err("harness.opencode: model must be provider/model with optional #variant".into());};
                if provider.is_empty() || name.is_empty() || name.contains('#') {return Err("harness.opencode: incomplete provider/model selection".into());}
                if let Some(effort)=&self.effort {*model=format!("{base}#{effort}");}
            }
            serde_json::Value::Object(model)=>{
                let provider=model.get("providerID").and_then(serde_json::Value::as_str).ok_or("harness.opencode: object selection needs providerID")?;
                let name=model.get("model").and_then(serde_json::Value::as_str).ok_or("harness.opencode: object selection needs model")?;
                if provider.is_empty() || provider.contains(['/', '#']) || name.is_empty() || name.contains('#') {return Err("harness.opencode: invalid object model selection".into());}
                if let Some(effort)=&self.effort {model.insert("variant".into(),serde_json::Value::String(effort.clone()));}
            }
            _=>return Err("harness.opencode: expected string or object model selection; inherited fields preserved".into()),
        }
        object.insert("model".into(), selected);
        serde_json::to_string(&config)
            .map(Some)
            .map_err(|e| e.to_string())
    }
}
