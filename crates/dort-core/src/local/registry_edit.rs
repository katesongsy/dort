use super::*;
use crate::{
    adoption::same,
    journal,
    observation::{self, Entry},
};

#[derive(Clone)]
pub enum Change {
    Startup(PathBuf),
    Forget(PathBuf),
    Reconnect {
        previous: PathBuf,
        destination: PathBuf,
    },
}

pub struct Preview {
    change: Change,
    repository_id: String,
    previous: Entry,
    destination: Option<Entry>,
}

pub struct Approval(Preview);

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

impl Change {
    fn root(&self) -> &Path {
        match self {
            Self::Startup(root) | Self::Forget(root) => root,
            Self::Reconnect { previous, .. } => previous,
        }
    }
}

impl Preview {
    pub fn prepare(host: &HostContext, change: Change) -> Result<Self> {
        let (config, previous) = LocalConfig::load_bound(host)?;
        let root = change.root();
        let repository_id = config
            .repositories
            .iter()
            .find(|(_, record)| record.root == root)
            .map(|(id, _)| id.clone())
            .ok_or_else(|| invalid("Enter the exact path of a registered repository."))?;
        let destination = match &change {
            Change::Startup(root) => {
                if inspect_repository(host, root)? != *root {
                    return Err(invalid(
                        "Reconnect the repository before making it the startup default.",
                    ));
                }
                Some(observation::read_manifest(&root.join("dort.toml"))?.1)
            }
            Change::Forget(_) => None,
            Change::Reconnect {
                previous,
                destination,
            } => {
                observation::validate_output_path(previous)?;
                observation::validate_output_path(destination)?;
                if observation::inspect(previous, false)?.kind != observation::Kind::Missing {
                    return Err(invalid(
                        "The previous repository path still exists. Reconnect only after a move.",
                    ));
                }
                if inspect_repository(host, destination)? != *destination
                    || config
                        .repositories
                        .values()
                        .any(|record| record.root == *destination)
                {
                    return Err(invalid(
                        "Choose a physical path to a repository that is not registered.",
                    ));
                }
                Some(observation::read_manifest(&destination.join("dort.toml"))?.1)
            }
        };
        Ok(Self {
            change,
            repository_id,
            previous,
            destination,
        })
    }

    pub fn human(&self) -> String {
        let message = match &self.change {
            Change::Startup(root) => {
                format!("Startup repository: {root:?}.\nDort will open here next time.")
            }
            Change::Forget(root) => format!(
                "Forget registration for {root:?}.\nKeep the repository, Home files, profile choices and history. Clear the startup choice if it points here."
            ),
            Change::Reconnect {
                previous,
                destination,
            } => format!(
                "Reconnect {previous:?} at {destination:?}.\nKeep the repository ID, saved choices and history. Existing Home links may still point to the old folder."
            ),
        };
        format!("{message}\n\nOnly local registration changes.")
    }

    pub fn approve(self) -> Approval {
        Approval(self)
    }
}

impl Approval {
    pub fn execute(self, host: &HostContext) -> Result<Option<PathBuf>> {
        let lock = WriterLock::acquire(host)?;
        journal::ensure_writable(host)?;
        let p = self.0;
        let fresh = Preview::prepare(host, p.change.clone())?;
        if p.repository_id != fresh.repository_id
            || !same(&p.previous, &fresh.previous, true)
            || match (&p.destination, &fresh.destination) {
                (Some(a), Some(b)) => {
                    !same(a, b, true)
                        || a.clone().stable_parents().parents != b.clone().stable_parents().parents
                }
                (None, None) => false,
                _ => true,
            }
        {
            return Err(invalid("Registration or repository changed. Review again."));
        }
        let (mut config, entry) = LocalConfig::load_bound(host)?;
        if !same(&p.previous, &entry, true) {
            return Err(invalid("Registration changed. Review again."));
        }
        let new_selection = match p.change {
            Change::Startup(root) => {
                config.active_repository = Some(p.repository_id);
                Some(root)
            }
            Change::Forget(_) => {
                config.repositories.remove(&p.repository_id);
                if config.active_repository.as_ref() == Some(&p.repository_id) {
                    config.active_repository = None;
                }
                None
            }
            Change::Reconnect { destination, .. } => {
                config.repositories.insert(
                    p.repository_id,
                    RepositoryRecord {
                        root: destination.clone(),
                    },
                );
                Some(destination)
            }
        };
        config.save(host, &lock)?;
        Ok(new_selection)
    }
}
