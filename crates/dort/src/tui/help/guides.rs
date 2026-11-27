use super::{Comparison, Topic};
use crate::tui::components::help::{Guide, Section};

macro_rules! guide {
    ($title:literal, $intro:expr; $($label:literal => $text:expr),* $(,)?) => {
        &Guide { title: $title, intro: $intro, sections: &[$(Section { label: $label, text: $text }),*] }
    };
}

const DEFAULTS: &str = "Press Space on a row to toggle it. Default for all hosts enables the profile everywhere. Default on macOS and Default on Linux enable it on that platform. Leave all three off for an optional profile.";
const MAPPING_SOURCE: &str = "Enter a path inside the repository, such as files/editor.conf. Leave off the leading slash. Avoid .git, manifest files and .. path segments.";
const MAPPING_TARGET: &str = "Enter the path where the link should appear in your home folder, such as ~/.config/editor.conf. Use ~/ or the full path under your home folder.";
const MAPPING_PROFILES: &str = "Open Profiles with Enter or Space. Toggle profile names with Space or a click, then press Enter. Blank uses base.";
const MAPPING_TOOLS: &str = "Open Required tools to select declared tools. Leave it blank if the file needs no tools. Review the environment before applying the link.";
const MAPPING_OVERRIDES: &str = "macOS source and Linux source can each name a different path inside the repository. Leave either blank to use Repository source on that platform.";
const ADOPT_EFFECTS: &str = "Dort copies the file into the repository, adds a mapping to the manifest and replaces the original with a link to the copy. Review those paths and the backup before confirming. A link makes the repository file available at the old path.";
const CHECK_EFFECTS: &str = "Package managers can run their configuration and plugins and change caches. The check does not install packages.";
const TOOL_COMMAND: &str = "Enter the executable name, such as rg. Use a name without folders or arguments. It can differ from Package.";
const TOOL_DEPENDENCIES: &str = "Press F4 for Dependencies. Open Required tools to select other declared tools. For npm, open npm runtime tool and choose the declared Node tool.";
const TOOL_PACKAGE: &str = "Choose Homebrew or npm. Package is the formula name, such as ripgrep, or npm package name, such as @scope/name. Leave Version blank for the package manager's default. Both accept latest. Only npm accepts an exact version such as 1.2.3.";
const SEARCH_FOLDERS: &str = "Enter absolute folder paths separated by colons, such as /opt/homebrew/bin:/usr/bin. Dort searches them for commands.";
const TEMP_PROFILES: &str = "Base and other mandatory profiles stay on. Use Space to add or remove optional profiles, then Enter to validate the selection. Esc keeps the old choices.";
const REVIEW_TAIL: &str = "Scroll to the end to see the approval action. If the review is blocked or Preview only, read the reason and go back to change the inputs.";
const GIT_PATCH: &str = "Select an entry and press Enter to open or focus its patch. Use F3 for metadata such as the path or rename. Dort reads only the selected preview and drops it when you leave. Git inspection makes no changes.";

impl Topic {
    pub(super) fn guide(self) -> &'static Guide {
        match self {
            Self::Overview => {
                guide!("Overview", "Start with a repository, the folder Dort uses to store and share your settings. Choose repository (Ctrl-P) to open one, or use Create/attach to set one up.";
                    "Bring a file into Dort" => "Open Files, then Discover. Select a regular file to adopt. Dort shows where it will copy the file and what link will replace the original before you approve.",
                    "Find other work" => "Use Tools to check or install packages listed by the repository. Use Git to see repository changes. Use Recovery to inspect earlier operations and see what you can undo.",
                    "Use a repository you already have" => "Open Profiles to choose groups of files and tools. Open Checks to validate the manifest, the repository's dort.toml file. Review environment to see tool and file changes for this machine.",
                )
            }
            Self::OverviewDetails => {
                guide!("Repository details", "Check which folder Dort is using, how it was selected and which profiles are active. Read the file counts before reviewing a change.";
                    "Read the counts" => "Linked files already point to the expected source. Needs apply counts links that need work. Blocked mappings have a conflict. Open Files to see the reasons, or press F3 for Summary.",
                )
            }
            Self::Discover => {
                guide!("Discover files", "Find files in your home folder. Pick a regular file and choose Adopt selected to prepare a draft. You can browse without a repository. Adoption needs one.";
                    "Read the list" => "File means regular file. Dir is a folder and Link points to another path. A folder groups entries; open or close it with Enter. Check Details for why a selected file can or cannot be adopted.",
                    "Work on several files" => "Press Space to mark a file or the eligible files in a folder. Marks survive filtering and folding. Open F2 More for Adopt marked or to clear marks. Check the hidden mark count before reviewing.",
                    "Hide a path" => "Use Ignore selected in More to hide the file, or a folder and its descendants, from discovery on this machine. Use Ignored paths to undo it.",
                )
            }
            Self::DiscoverDetails => {
                guide!("Discovered file details", "Read the selected path, file type and eligibility. A regular file can be adopted if it passes Dort's safety checks. Read the reason if adoption is blocked.";
                    "Next action" => "Adopt selected opens an editable draft. Ignore selected hides the path from discovery on this machine after review. Esc returns to the list.",
                )
            }
            Self::Content => {
                guide!("Selected file contents", "Read the selected file here. Following link shows the path Dort opened for a supported link. Use the arrows or page keys to read more, or Esc to return to the file list.";
                    "What stays private" => "The preview is temporary and can be unavailable for unsafe, unreadable or unsupported files. Its contents do not appear in Help or reports.",
                    "Adopt after reading" => "Use Adopt selected when ready. The draft lets you change the destination, mapping ID, repository source and profiles before review.",
                )
            }
            Self::ManagedContent => {
                guide!("Managed file contents", "Read the file at the selected mapping's destination. Following link names the file Dort opened. Use the arrows or page keys to read, or Esc to return to the list.";
                    "Check what Dort manages" => "Read Details for the repository source, destination and link status. The temporary preview does not put file contents in Help or reports.",
                )
            }
            Self::Managed => {
                guide!("Managed files", "A mapping tells Dort which repository file to link into your home folder. Select a mapping and press Enter to inspect its source, destination and status.";
                    "Read status" => "Linked means the destination points to the expected source. Needs apply means it is not in place yet. Blocked means Dort found a conflict. Read the reason in Details before applying.",
                    "Generated files" => "Press s for New generated file. Select a generated file and press v to preview it, or e to edit a template saved in dort.toml.",
                    "Choose which files to apply" => "Apply reviews the active profiles, even with a filter on screen. To review a subset, press Space to mark the mappings and use Apply marked. Marks can be hidden by the filter.",
                    "Change what Dort manages" => "Add file mapping or Edit file mapping saves a declaration for later apply. Unmanage leaves a regular copy at the destination. Delete link removes the link at the destination. Both keep the repository source.",
                )
            }
            Self::ManagedDetails => {
                guide!("Managed file details", "Compare Source in the repository with Destination in your home folder. Linked means the destination points to the expected source. If Blocked, read Reason and check the two file types.";
                    "Choose a change" => "Edit file mapping changes the declaration. Review environment checks required tools before you apply. Unmanage leaves a regular copy at the destination. Delete link removes the link. Both keep the source.",
                    "Return to the list" => "Press Esc to choose a different mapping or mark a group. Apply marked uses the marks. Filtering alone never limits Apply.",
                )
            }
            Self::GeneratedDraft => {
                guide!("Generated file draft", "Set the Home destination and template. Use {{ name }} for inputs. Tab moves between fields. In Template, Enter adds a line. Paste several lines at once.";
                    "Open in editor" => "Press F5 to use VISUAL or EDITOR. Save and exit the editor to return to the draft.",
                    "Enter inputs" => "Press F4 to continue. Empty values and spaces are kept. Esc goes back without losing values. Never enter secrets.",
                    "Review before saving" => "Review generated file shows the rendered text. Save generated file only updates dort.toml. Your Home files stay in place.",
                )
            }
            Self::Generated => {
                guide!("Generated file preview", "Read the template output and its destination status. Inputs come from dort.toml. No files are written.";
                    "Read the output" => "Use arrows or page keys to scroll. Home goes to the top; End goes to the last line. Esc returns to Managed.",
                    "Check the status" => "Dort shows template updates and local changes separately. The preview leaves the deployed file alone.",
                    "Deploy for the first time" => "For a new file, choose Review deployment (R). Press y when Deploy generated file appears. Esc cancels the review.",
                )
            }
            Self::Recovery => {
                guide!("Recovery", "Select an operation record and press Enter to inspect its state and available recovery actions. This list includes records from other repositories.";
                    "Pick a recovery action" => "Rollback reviews which files can be restored. Resume continues recoverable setup. Abandon closes incomplete setup and keeps what was published, including private files. Read the reason if an action is blocked.",
                    "Check current files" => "Dort checks paths and refuses to restore files that no longer match the expected state. Rollback does not uninstall packages. Backups are private files, not encrypted.",
                )
            }
            Self::RecoveryDetails => {
                guide!("Operation details", "Read Status and State to see where the operation stopped. Recovery options lists the actions available for this record. Read Blocked or Next when shown.";
                    "Before review" => "Check the paths and the record before choosing Rollback, Resume or Abandon. Dort refuses to restore changed files. Abandon keeps published files and private backups.",
                )
            }
            Self::Checks => {
                guide!("Manifest checks", "Choose Check repository to validate dort.toml for the current repository, or Standalone check to use a separate manifest. A manifest lists the files, tools and profiles Dort should use.";
                    "What a check covers" => "Dort checks the declarations and counts the files and tools selected by the active profiles. If it offers a package manager check, read and approve that part separately. Use Files for live file state or Diagnostics for path and host problems.",
                )
            }
            Self::CheckResults => {
                guide!("Manifest check results", "Valid means Dort accepted the declarations. Selected files and Selected tools count what your active profiles include. Use Check again after changing the manifest or profiles.";
                    "Read the results" => "Check Scope to see which manifest was checked. Effective profiles lists the groups in use. Unavailable lists features Dort cannot use here. A tool result appears only for tools included in the check.",
                    "Next steps" => "Open Files for live link status. Choose Review environment to see proposed changes on this machine.",
                )
            }
            Self::Diagnostics => {
                guide!("Diagnostics", "Check the detected host, Git, paths and repository state when a command is blocked or uses the wrong folder. Choose Refresh diagnostics to read them again.";
                    "What to fix" => "Check the session repository and manifest path first. Read any history or write warning before trying another change. If recovery is required, open Recovery and inspect the record.",
                )
            }
            Self::Git(Comparison::Status) => {
                guide!("Git status", "See which paths changed in the Git repository. Pick a row to compare Staged changes prepared for a commit with Unstaged changes still in the working files. Untracked means Git has not added the path.";
                    "Read one entry" => GIT_PATCH,
                    "Choose the repository" => "Use More to choose a Git root or return to the current file repository. Refresh after changing files outside Dort.",
                )
            }
            Self::Git(Comparison::Unstaged) => {
                guide!("Git unstaged changes", "See changes in the working files that are not staged for the next commit. Select a path to see what changed.";
                    "Read one entry" => GIT_PATCH,
                )
            }
            Self::Git(Comparison::Staged) => {
                guide!("Git staged changes", "See changes already staged for the next commit. Select a path to compare the staged copy with the last commit.";
                    "Read one entry" => GIT_PATCH,
                )
            }
            Self::GitRoot => {
                guide!("Choose a Git root", "Enter the repository folder in Exact Git root, such as /home/you/dotfiles. Relative paths start at the folder you launched Dort from. Choose Read Git to inspect it.";
                    "Check the scope" => "F3 shows the full path you typed. The choice lasts for this session. It changes the Git page only, not the repository Dort uses for files.",
                )
            }
            Self::GitPatch => {
                guide!("Git patch", "Read the changes for the selected path. In the change lines, + means added and - means removed.";
                    "Navigate" => "Use the arrows or page keys to scroll. Press F3 for metadata or Esc to return to the entries. The temporary patch is not included in Help or saved reports.",
                )
            }
            Self::GitMetadata => {
                guide!("Git entry details", "Read the selected path and any rename or status information reported by Git. This view describes the entry; it does not stage or change anything.";
                    "See the patch" => "Press Esc to return to the entries, then Enter to open the patch if available.",
                )
            }
            Self::Declared => {
                guide!("Declared tools", "Review tools your repository lists in dort.toml. Select a tool and press Enter for details, or choose Check installed to review which package managers Dort will query.";
                    "Read status" => "Not checked or Check needed means you need a current check. Inactive profile means the tool is outside your active groups. Installed means the package manager reports the package. Read Tool details to see whether its command was found.",
                    "Install or edit" => "Use Install missing after a current check to review eligible tools. To change what is declared without installing anything, open Tool details or use More to add a declaration. Installed and History are separate Tools pages.",
                )
            }
            Self::DeclaredDetails => {
                guide!("Tool details", "Review the selected tool's command, package, version and profiles. Package status comes from the package manager. Command status says whether Dort found the executable in the checked folders.";
                    "Act on the result" => "Missing means no package was reported. Version differs means Dort found a different version. If it couldn't check, use Tool settings to inspect the program and search folders, then check again.",
                    "Change the declaration" => "Edit declaration and Remove declaration change the manifest after review. Neither changes existing installed packages.",
                )
            }
            Self::ToolAdd => {
                guide!("Add tool declaration", "Record a tool in the manifest, the repository's dort.toml. Enter a unique Tool ID such as search, then fill in Command name and choose the Package manager.";
                    "Command name" => TOOL_COMMAND,
                    "Profiles" => MAPPING_PROFILES,
                    "Package manager, Package and Version" => TOOL_PACKAGE,
                    "Required tools and npm runtime tool" => TOOL_DEPENDENCIES,
                    "Save the declaration" => "Choose Review declaration to see the manifest change. This does not install a package. Use Check installed later to see what you have.",
                )
            }
            Self::ToolEdit => {
                guide!("Edit tool declaration", "Change the selected tool's declaration. The Tool ID stays fixed. Start with the existing fields and choose Review declaration to review what will be saved.";
                    "Command name" => TOOL_COMMAND,
                    "Profiles" => MAPPING_PROFILES,
                    "Package manager, Package and Version" => TOOL_PACKAGE,
                    "Required tools and npm runtime tool" => TOOL_DEPENDENCIES,
                    "What changes" => "Saving edits the manifest. It does not remove or change an installed package. Check again to get status for the new declaration.",
                )
            }
            Self::ToolRemove => {
                guide!("Remove tool declaration", "Choose Review removal to see the exact manifest change. Dort drops the selected declaration only after approval. Packages already installed stay in place.";
                    "Before removing" => "Check whether files or other tools still require this tool. If they do, update those declarations first.",
                )
            }
            Self::BrewSettings => {
                guide!("Homebrew tool settings", "Set which Homebrew program Dort checks for this tool. In Package manager program, enter the absolute path to brew, such as /opt/homebrew/bin/brew.";
                    "Command search folders" => SEARCH_FOLDERS,
                    "Save on this host" => "The form starts with the current paths. Saving affects this tool in this repository on this machine. Validate paths in dry run checks the inputs without saving.",
                )
            }
            Self::NpmSettings => {
                guide!("npm tool settings", "Set which Node and npm Dort use for global packages. In Node program, enter the absolute path to the node executable.";
                    "Command search folders" => SEARCH_FOLDERS,
                    "npm CLI script and Global package prefix" => "Enter the full path to npm-cli.js and the full folder path used by npm for global packages. The prefix is the folder that contains global bin and package directories, not the npm program.",
                    "Save on this host" => "Saving affects tools that share this Node runtime in this repository on this machine. Validate paths in dry run checks the inputs without saving.",
                )
            }
            Self::ToolManifest => {
                guide!("Check tools in a manifest", "Enter an Absolute manifest path, such as /home/you/dotfiles/dort.toml. Choose Review checks to see which packages and commands Dort will look for.";
                    "Independent check" => "The result does not select a repository or prepare installs. To install tools, return to Declared tools in the correct repository and check that repository instead.",
                )
            }
            Self::ToolCheck => {
                guide!("Review installed checks", "Read the tool list and the package manager programs and paths. Choose Run checks when the review shows the scope you want to check.";
                    "What runs" => CHECK_EFFECTS,
                    "After the check" => "Read the package and command results separately. Open Tool settings and check again if a required program or path is unavailable.",
                )
            }
            Self::StandaloneToolCheck => {
                guide!("Review standalone tool check", "Check the manifest path and the package manager commands. Choose Run checks to inspect the tools listed for this standalone check.";
                    "What runs" => CHECK_EFFECTS,
                    "Use the results" => "The results describe this manifest only. They cannot be used for Install missing.",
                )
            }
            Self::StandaloneToolResults => {
                guide!("Standalone check results", "Read the status next to each selected tool. Installed means the package manager reports it. Missing, Version differs and Couldn't check tell you where to look next.";
                    "Continue" => "Choose Done to go back. To prepare installs, choose a repository and run Check installed on its Declared tools page.",
                )
            }
            Self::InstallReview => {
                guide!("Review missing installs", "Read the exact list of eligible tools and the package manager commands Dort will run. Check the package names and versions before choosing whether to install.";
                    "Before you approve" => "Installers can change global packages and run package code. Dort installs in the listed order and stops on the first failure. Earlier installs stay in place.",
                    "After the install" => "Review the outcome for each tool and check installed again before another install.",
                )
            }
            Self::InstallBlocked => {
                guide!("Installation needs attention", "No tools are ready to install from this review. Read the reason and next step for each blocked tool.";
                    "Start a new review" => "Go back, fix the listed issues and use Check installed again before requesting another install review.",
                )
            }
            Self::InstallResults => {
                guide!("Install results", "Read Outcome, Package and Command for each tool. Not started names the tools left after a failure. Earlier installs stay in place.";
                    "Continue" => "Choose Done. Run Check installed again before another install. Use History to inspect an installation record if the result shows one.",
                )
            }
            Self::InstalledReview => {
                guide!("Check installed packages", "Check the listed Homebrew and npm commands and paths first. Approve the check to see packages the available managers report on this machine.";
                    "What runs" => CHECK_EFFECTS,
                    "What you get" => "Installed packages is a temporary result. If a manager is unavailable, read the reason in the review. A missing result is not proof that no packages are installed.",
                )
            }
            Self::Installed => {
                guide!("Installed packages", "Browse packages reported by Homebrew or npm on this machine. Select a package and press Enter for Package details. Use Filter packages (/) to narrow the list.";
                    "Use the findings" => "A package report does not prove that its command works. Check installed again to refresh. Add declaration fills known fields for review without installing anything.",
                )
            }
            Self::InstalledDetails => {
                guide!("Package details", "Read the selected package name, manager and reported versions. This is the result of the last check, not a test that its command runs.";
                    "Declare it in your repository" => "Add declaration opens a draft with the known package fields filled in. Check Tool ID and Command name before reviewing. Nothing is installed by saving the declaration.",
                )
            }
            Self::InstallHistory => {
                guide!("Installation history", "Select an installation attempt and press Enter for Install record. Records describe what Dort tried to install, not all packages on the machine.";
                    "Unresolved attempts" => "Open the record to see what Dort observed. Use Close record when offered to review a fresh check without rerunning the install.",
                )
            }
            Self::InstallRecord => {
                guide!("Install record", "Read the package, command and recorded outcome. A stopped or unresolved record does not prove the package was left unchanged.";
                    "Close a record" => "Close record prepares a fresh check of current package state. It does not run the installer or remove packages. If the original inputs are damaged, Dort asks for the package manager paths.",
                )
            }
            Self::CloseBrewRecord => {
                guide!("Close a Homebrew record", "Give Dort the paths for a fresh check. Package manager program is the absolute path to brew. Change package manager (F4) if this was an npm install.";
                    "Command search directories" => SEARCH_FOLDERS,
                    "Homebrew prefix" => "Enter the full Homebrew installation folder, such as /opt/homebrew. Choose Review close to inspect the check. Closing the record never retries the install.",
                )
            }
            Self::CloseNpmRecord => {
                guide!("Close an npm record", "Give Dort the paths for a fresh check. Node program is the absolute path to node. Change package manager (F4) if this was a Homebrew install.";
                    "Command search directories" => SEARCH_FOLDERS,
                    "npm CLI script and Global prefix" => "Enter the full path to npm-cli.js and the full folder path for npm's global packages. Choose Review close to inspect the check. Closing the record never retries the install.",
                )
            }
            Self::CloseRecordReview => {
                guide!("Review closing a record", "Check the program, arguments and paths. Approval runs a fresh check and records what it finds. The original installer is not retried.";
                    "What runs" => CHECK_EFFECTS,
                )
            }
            Self::More => {
                guide!("More actions", "Use the arrows to select an action and Enter to open it. Press F3 for details about the selected action, including why it may be unavailable.";
                    "Check which items it affects" => "An item action uses the selected row. A marked action uses all the marks, including hidden rows. A view action belongs to this page. Review a change before approving it.",
                )
            }
            Self::ProfileChoice => {
                guide!("Choose saved profile setting", "Choose how this profile behaves on this machine. Use defaults follows the repository and platform defaults. On enables the group here. Off disables it here.";
                    "Before saving" => "Enter opens a review of the local setting. It does not edit dort.toml. Esc returns with the current setting unchanged.",
                )
            }
            Self::DraftReferences => {
                guide!("Choose profiles or tools", "Use the arrows to move. For a list with multiple selections, press Space or click a row to toggle it.";
                    "Apply to the draft" => "Enter applies the selection to the draft. Esc keeps the draft unchanged. Review the form separately before saving.",
                    "Blank profiles" => "No selected profiles means base.",
                )
            }
            Self::DeclarationProvider => {
                guide!("Choose a package manager", "Choose Homebrew for a formula or npm for a global package. Press Enter to return to the declaration and fill in Package and Version.";
                    "What changes" => "The selection changes the draft only. For npm, also choose the declared Node tool in npm runtime tool.",
                )
            }
            Self::CloseRecordProvider => {
                guide!("Choose a manager for the record", "Choose the package manager used for the original install. Enter returns to the form for its program and paths.";
                    "What changes" => "The manager choice fills the draft only. Review close shows the check before it runs. It never reruns the installer.",
                )
            }
            Self::Adopt => {
                guide!("Adopt a file", "Bring one regular file in your home folder under Dort. Check the suggested paths if you came from Discover, or fill them in before choosing Review read-only preview.";
                    "TARGET" => MAPPING_TARGET,
                    "ID" => "Enter a unique mapping name such as editor-config. Start with a letter or number. Use letters, numbers, dots, underscores or hyphens.",
                    "SOURCE" => MAPPING_SOURCE,
                    "Profiles" => MAPPING_PROFILES,
                    "Before confirming" => ADOPT_EFFECTS,
                    "Draft controls" => "Use Tab between fields. Repo chooses a repository, New opens setup and Reset clears this draft. F3 shows field values in Details.",
                )
            }
            Self::AddMapping => {
                guide!("Add file mapping", "Declare a link to a repository file. Enter a unique Mapping ID such as editor-config, then set Repository source and Home destination.";
                    "Repository source" => MAPPING_SOURCE,
                    "Home destination" => MAPPING_TARGET,
                    "Profiles" => MAPPING_PROFILES,
                    "Required tools" => MAPPING_TOOLS,
                    "Platform sources" => MAPPING_OVERRIDES,
                    "What happens next" => "Choose Review changes to see the manifest edit. Saving does not create the link. Review Apply later to put it in place.",
                )
            }
            Self::EditMapping => {
                guide!("Edit file mapping", "Change where an existing mapping reads from, where it links, or when it applies. The Mapping ID stays fixed. The fields start with the saved declaration.";
                    "Repository source" => MAPPING_SOURCE,
                    "Home destination" => MAPPING_TARGET,
                    "Profiles" => MAPPING_PROFILES,
                    "Required tools" => MAPPING_TOOLS,
                    "Platform sources" => MAPPING_OVERRIDES,
                    "What happens next" => "If a link is already in place and you need to change its source or destination, use Unmanage first, then add the mapping again. Review changes opens the proposed declaration edit.",
                )
            }
            Self::Startup => {
                guide!("Set startup repository", "Enter a Registered repository path, such as /home/you/dotfiles. Use the exact folder path shown in Choose repository. Choose Review changes to check what Dort will save.";
                    "When it takes effect" => "Dort remembers this preference on this machine for future launches. It does not move the repository or edit its manifest.",
                )
            }
            Self::Forget => {
                guide!("Forget repository", "Enter the exact Registered repository path to remove from this machine's list. Choose Review changes to see what will be removed from the registry.";
                    "What stays" => "Dort keeps the repository, its files and the links. If you forget the startup repository, Dort clears the saved startup choice too.",
                )
            }
            Self::Reconnect => {
                guide!("Reconnect moved repository", "Update Dort's saved path after moving a repository. Previous registered path is the old exact folder. New repository path is the full path to the repository at its new location.";
                    "Before saving" => "Choose Review changes to verify the new location and see the local registration change. Reconnect does not move files or repair links pointing at the old location. Check Files afterward.",
                )
            }
            Self::OpenRepair => {
                guide!("Open a manifest repair draft", "Enter the Exact unregistered Git repository path. Use the full path to the folder with the broken dort.toml. Git must be available.";
                    "What happens next" => "Open repair draft loads editable lines. Review the diff before saving. If you want to attach the repaired repository, review that separately afterward.",
                )
            }
            Self::Repair => {
                guide!("Repair manifest", "Edit the numbered Line fields to fix dort.toml. Tab moves between lines and F4 inserts a line. Choose Review manifest diff to check the changes before saving.";
                    "Before saving" => "Check the diff, including any sections that were not meant to change. Saving changes the manifest only. Attaching the repository is a separate review.",
                )
            }
            Self::CheckForm => {
                guide!("Check a standalone manifest", "Enter Manifest path for a separate dort.toml file, such as /home/you/dotfiles/dort.toml. Choose Check manifest to validate its declarations.";
                    "Profiles for this check" => "Base is always active. Use Profiles (Ctrl-P) to add named groups for this check. Reset (Ctrl-D) clears the draft. A standalone check does not switch your repository.",
                )
            }
            Self::ChoosePath => {
                guide!("Enter a repository path", "In Path, enter the root folder that contains dort.toml, not the file itself. Use an absolute path or a path relative to where you started Dort.";
                    "Select or create" => "Select repository opens that folder for this session. If you need to create or attach one first, choose New (Ctrl-N) for the setup draft.",
                )
            }
            Self::Clone => {
                guide!("Clone repository", "In Git source, enter an HTTPS Git URL or absolute local Git path. In New folder, enter the destination for the clone. Choose an unused folder name.";
                    "Review clone and registration" => "New folder can be absolute or relative to where you launched Dort. Check both paths. Approval clones and registers a valid Dort repository. If its manifest is invalid, repair may be offered before registration.",
                )
            }
            Self::Init => {
                guide!("Create or attach a repository", "Enter the repository folder in Path. To create one, give an unused path with an existing parent folder. To attach one, choose a Git repository with a valid dort.toml.";
                    "Review before creating" => "Git must be available. Relative paths start where you launched Dort. Choose Review read-only preview to see what setup will create or register. Use Repo (Ctrl-R) to choose a different repository instead.",
                )
            }
            Self::KeepId => {
                guide!("Unmanage and keep a local file", "In Exact ID, type the mapping name as shown in Managed. Choose Review read-only preview to check the destination before taking it out of Dort.";
                    "What stays" => "Dort keeps the repository source and leaves a regular copy at the destination in place of the link.",
                )
            }
            Self::RemoveId => {
                guide!("Delete a managed link", "In Exact ID, type the mapping name as shown in Managed. Choose Review read-only preview to check the exact link to remove.";
                    "What stays" => "The repository source stays. Dort removes the managed link without leaving a regular file at the destination. Use Unmanage if you want a local copy.",
                )
            }
            Self::CreateProfile => {
                guide!("Create profile", "Enter a new name such as work in Profile name. The name identifies a group of files and tools. Base is reserved and stays on.";
                    "Defaults" => DEFAULTS,
                    "After saving" => "Choose Review changes to check the new group. Assign files and tools to it separately in their declarations.",
                )
            }
            Self::EditProfileDefaults => {
                guide!("Edit profile defaults", "Change when the selected profile turns on by default. The rows start with the saved values. Choose Review changes to see the manifest edit.";
                    "Defaults" => DEFAULTS,
                    "This machine only" => "Use Local choice on Profiles if you only want to change the setting on this machine.",
                )
            }
            Self::RenameProfile => {
                guide!("Rename profile", "Enter a new, unused name in New name for your selected profile. For example, enter office instead of work.";
                    "Before saving" => "Review changes shows the new name and changes to files, tools and defaults that use it. Check Local choice under the new name afterward.",
                )
            }
            Self::DeleteProfile => {
                guide!("Delete profile", "Remove a profile from the manifest only after files and tools stop using it. Base cannot be deleted.";
                    "Before saving" => "Check the name in the review. Dort keeps the files and tools; the profile is removed from the defaults as well as the profile list.",
                )
            }
            Self::Profiles => {
                guide!("Profiles", "A profile is a named group of files and tools. Base is always on. Select a profile to see whether it is active and which defaults or local choice apply.";
                    "Choose where it applies" => "Local choice saves an override on this machine. Edit profile defaults updates the repository for all hosts or a platform. Temporary profiles adds groups for this session only.",
                    "Manage profile names" => "Use New profile to create a group. Rename profile updates declarations that use the old name. Delete profile is available only for unused groups.",
                )
            }
            Self::Repositories => {
                guide!("Choose repository", "Select a registered folder and press Enter to use it for this session. To open a different folder, choose Enter exact repository path.";
                    "Set up or use the saved choice" => "Use registered active repository opens the saved local choice if valid. Create/attach opens a draft for a separate setup review. Choosing a repository here does not change the startup preference.",
                )
            }
            Self::TemporaryProfiles => {
                guide!("Temporary profiles", "Add groups of files and tools for this session. The list shows which profiles you can add and which are already mandatory.";
                    "Choose additions" => TEMP_PROFILES,
                    "Save a local preference instead" => "To make a choice that lasts after you exit, open Profiles and use Local choice.",
                )
            }
            Self::StandaloneProfiles => {
                guide!("Standalone profiles", "Choose extra groups for this standalone manifest check. These choices are for the check only. Your repository and its profiles stay as they were.";
                    "Choose additions" => TEMP_PROFILES,
                )
            }
            Self::Ignored => {
                guide!("Ignored paths", "Select a path and inspect it before choosing Unignore. Dort saves ignored paths on this machine. A folder entry hides its descendants too.";
                    "What changes" => "Unignore removes the discovery preference after review. The file or folder stays where it is. Return to Discover to find it again if it is eligible to be shown.",
                )
            }
            Self::About => {
                guide!("About Dort", "Find the version and supported hosts here. Start in Overview to choose a repository, or use Files and Discover to see what Dort can manage.";
                    "Limits" => "Dort handles regular-file links, Homebrew and npm tools, and read-only Git inspection. Git writes, secrets, templates and sync are outside this app.",
                )
            }
            Self::Batch => {
                guide!("Adopt marked files", "Check the suggested target, ID and source for each marked file. Select Edit item to change one draft, or Review all to check the whole batch.";
                    "Read the list" => "TARGET is the path in your home folder, ID identifies the mapping and SOURCE is the path inside the repository. Open Details for the full values, including profiles.",
                    "Before confirming" => ADOPT_EFFECTS,
                )
            }
            Self::BatchItem => {
                guide!("Edit a batch item", "Change one file's draft and choose Done editing to return to the batch. This does not adopt the file. Use Review all on the batch screen when ready.";
                    "Target" => MAPPING_TARGET,
                    "Mapping ID" => "Use a unique name such as editor-config. Do not reuse another item's ID.",
                    "Repository source" => MAPPING_SOURCE,
                    "Profiles" => "Enter existing profile names separated by commas. Blank uses base.",
                )
            }
            Self::Preview => {
                guide!("Review proposed changes", "Read what Dort plans to change and verify the paths. Check the listed backups and any changes to the manifest before you approve.";
                    "See every effect" => "If you see a summary, choose Full effects (F3) to see each change. Press End to reach the last effect. Close Help to finish reading the review.",
                    "Finish reading" => REVIEW_TAIL,
                    "Keep your draft" => "Go back with Esc to revise the inputs. Approving the review is a separate step.",
                )
            }
            Self::Environment => {
                guide!("Review environment", "Read the tool and home file plan for the active profiles. Check the status and next action for each item before starting the run.";
                    "Resolve blocks first" => "If a file needs a tool that has not been checked, run Environment check and review again. Read unavailable or blocked items before deciding what to change.",
                    "Before approving" => "Earlier tool installs or file changes may remain if a later step fails. Review the run record before resuming. Scroll to the end to find Run environment when ready.",
                )
            }
            Self::EnvironmentCheck => {
                guide!("Review environment check", "Check the package managers and paths listed for this environment. Choose Run checks to save the check results for this repository and profiles.";
                    "What runs" => CHECK_EFFECTS,
                    "Next step" => "Return to Review environment to inspect the file and tool decisions with the new results.",
                )
            }
            Self::Task => {
                guide!("Work in progress", "Read the current step and wait for its result. Dort reports completed steps when it knows them. There is no estimated percentage.";
                    "If you need to exit" => "An approved write runs to completion. An exit request waits for the active worker. Read the result before trying the operation again.",
                )
            }
            Self::Error => {
                guide!("When an action fails", "Read the problem and any next step shown. For a read, use Retry read when available. For a failed write, inspect the current state before attempting another change.";
                    "Check for partial effects" => "A failed write may have already changed files or packages. Open Recovery if Dort offers it. A retry that changes state needs a new review.",
                )
            }
            Self::Result => {
                guide!("Read the result", "Check the outcome and recorded effects. If Dort offers a next action, read it before leaving. Otherwise choose Done to return.";
                    "Follow up" => "Refresh if you need current file or repository details. If work stopped with partial effects, open Recovery when offered rather than repeating the same change.",
                )
            }
        }
    }
}
