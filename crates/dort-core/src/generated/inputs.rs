use super::local_inputs::Values;
use crate::{
    Error, Result,
    context::HostContext,
    model::{HostInput, ManagedFile},
    template::{self, Context, Input},
};
use serde::Serialize;
use std::{collections::BTreeMap, ffi::CStr, mem::MaybeUninit, ptr};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputProvenance {
    Manifest,
    Host(HostInput),
    MachineLocal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedInput {
    pub value: String,
    pub provenance: InputProvenance,
}

/// Names are qualified as `inputs.*`, `host.*` and `local.*`.
pub type ResolvedInputs = BTreeMap<String, ResolvedInput>;

impl HostInput {
    pub fn key(self) -> &'static str {
        match self {
            Self::Os => "os",
            Self::Arch => "arch",
            Self::User => "user",
            Self::Hostname => "hostname",
            Self::Home => "home",
            Self::ConfigHome => "config_home",
        }
    }

    fn resolve(self, host: &HostContext) -> Result<String> {
        let value = match self {
            Self::Os => Some(host.platform.as_str().into()),
            Self::Arch => Some(host.architecture.as_str().into()),
            Self::User => account_name(),
            Self::Hostname => hostname(),
            Self::Home => host.home.to_str().map(str::to_owned),
            Self::ConfigHome => host
                .config_dir
                .parent()
                .and_then(|path| path.to_str())
                .map(str::to_owned),
        };
        value.filter(|value| !value.is_empty()).ok_or_else(|| {
            let action = match self {
                Self::User => "Check the OS account for the effective user.",
                Self::Hostname => "Check the OS hostname.",
                Self::Home | Self::ConfigHome => "Use a UTF-8 directory path.",
                Self::Os | Self::Arch => unreachable!("resolved host platform and architecture"),
            };
            Error::Invalid(format!(
                "Cannot read host.{} as text. {action} Or remove it from host_inputs and the template.",
                self.key(),
            ))
        })
    }
}

fn account_name() -> Option<String> {
    let mut entry = MaybeUninit::<libc::passwd>::uninit();
    let mut buffer = vec![0; 256 * 1024];
    let mut result = ptr::null_mut();
    let uid = rustix::process::geteuid().as_raw();
    // On success with a non-null result, getpwuid_r initializes entry and puts the name in buffer.
    let status = unsafe {
        libc::getpwuid_r(
            uid,
            entry.as_mut_ptr(),
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut result,
        )
    };
    if status != 0 || result.is_null() {
        return None;
    }
    let name = unsafe { entry.assume_init().pw_name };
    if name.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(name) }
        .to_str()
        .ok()
        .map(str::to_owned)
}

fn hostname() -> Option<String> {
    let mut buffer = [0_u8; 256];
    // gethostname writes at most buffer.len() bytes. Check for a terminator after success.
    if unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) } != 0 {
        return None;
    }
    CStr::from_bytes_until_nul(&buffer)
        .ok()?
        .to_str()
        .ok()
        .map(str::to_owned)
}

pub(super) fn text_map<'a>(items: impl IntoIterator<Item = (&'a String, &'a String)>) -> Context {
    items
        .into_iter()
        .map(|(key, value)| (key.clone(), Input::Text(value.clone())))
        .collect()
}

/// Root names, except a name that a namespace owns. Those stay reachable as `inputs.name`.
pub(super) fn manifest_context(file: &ManagedFile) -> Context {
    let mut context = Context::new();
    let mut named_after_a_namespace = Context::new();
    for (name, value) in &file.inputs {
        let value = Input::Text(value.clone());
        if template::is_namespace(name) {
            named_after_a_namespace.insert(name.clone(), value);
        } else {
            context.insert(name.clone(), value);
        }
    }
    if !named_after_a_namespace.is_empty() {
        context.insert("inputs".into(), Input::Map(named_after_a_namespace));
    }
    context
}

pub(super) fn resolve(
    file: &ManagedFile,
    host: &HostContext,
    local: &Values,
) -> Result<(Context, ResolvedInputs)> {
    let mut resolved: ResolvedInputs = file
        .inputs
        .iter()
        .map(|(key, value)| {
            (
                format!("inputs.{key}"),
                ResolvedInput {
                    value: value.clone(),
                    provenance: InputProvenance::Manifest,
                },
            )
        })
        .collect();
    let mut context = manifest_context(file);
    if let Some(keys) = file.host_inputs {
        let mut values = Context::new();
        for key in keys.iter() {
            let value = key.resolve(host)?;
            values.insert(key.key().into(), Input::Text(value.clone()));
            resolved.insert(
                format!("host.{}", key.key()),
                ResolvedInput {
                    value,
                    provenance: InputProvenance::Host(key),
                },
            );
        }
        context.insert("host".into(), Input::Map(values));
    }
    if let Some(keys) = &file.local_inputs {
        let mut values = Context::new();
        for key in keys {
            if let Some(value) = local.get(key) {
                values.insert(key.clone(), Input::Text(value.clone()));
                resolved.insert(
                    format!("local.{key}"),
                    ResolvedInput {
                        value: value.clone(),
                        provenance: InputProvenance::MachineLocal,
                    },
                );
            }
        }
        context.insert("local".into(), Input::Map(values));
    }
    Ok((context, resolved))
}
