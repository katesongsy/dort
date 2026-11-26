# Changelog

Generated with git-cliff from committed Git history. Do not edit entries by hand.
Uncommitted changes are not included. Untagged history appears under Unreleased.

## v0.3.0 — 2026-11-26

### Documentation

- **verify:** cover form reference pickers ([e688e39](https://github.com/katesongsy/dort/commit/e688e39810f1f87fe5c4f1137cb67ebac6234444))

- **verify:** map the PF1 help journey ([37223c8](https://github.com/katesongsy/dort/commit/37223c877497aefb0525cf634013d444f2ed894e))

- **manifest:** state repair scope without negation ([246e4c2](https://github.com/katesongsy/dort/commit/246e4c256aa86fc8804111596ec66eb04f08c024))

### Features

- **tui:** pick declared profiles and tools in forms ([8fff2ff](https://github.com/katesongsy/dort/commit/8fff2ff5a95c8a51c17adb83e1ddddd25b678f80))

- **tui:** teach Dort workflows in PF1 help ([f7ce623](https://github.com/katesongsy/dort/commit/f7ce6233f47ccb232e434b80dfa5cd51d6c5283b))

- **tui:** clarify scope and validation feedback ([40813b3](https://github.com/katesongsy/dort/commit/40813b3660271003d80bb5b57b1fe6c6c52b5fcf))

- **tui:** put environment actions where users need them ([5994491](https://github.com/katesongsy/dort/commit/5994491094140f4e5b86404c3db5000441b2b56b))

- **tui:** choose profile defaults with toggle rows ([b679d09](https://github.com/katesongsy/dort/commit/b679d090b04d3051dd66f3b4e4bda9f7fd7e07b7))

- **tui:** split profile actions into named forms with field help ([d9420ca](https://github.com/katesongsy/dort/commit/d9420cab5a6c9be43cddd37fd4832379633edea9))

- **tui:** repair unregistered manifests with a reviewed diff ([1501348](https://github.com/katesongsy/dort/commit/1501348daaf0e27e52cf71b81af65351145f7f77))

- **tui:** check standalone tools and remember tool settings ([9508abb](https://github.com/katesongsy/dort/commit/9508abb4194b342d55fd6b4aa5c9c7404626801c))

- **tui:** manage registered repositories ([294cfed](https://github.com/katesongsy/dort/commit/294cfedf83fc2eaeafdfe1cfa6b9327607ad1782))

- **tui:** define profiles and edit file mappings ([5748766](https://github.com/katesongsy/dort/commit/574876608eab8ea235e8235c964507f9625e4ae3))

- **environment:** add saved profiles, clone, and resumable plans ([eff286a](https://github.com/katesongsy/dort/commit/eff286a3aa04420ae092529478eb537941c1d7b7))

### Fixes

- **environment:** review and preserve abandoned runs ([77bc998](https://github.com/katesongsy/dort/commit/77bc99861527f8af4c537b649d158c439c0f4c25))

- **tui:** let temporary profiles resolve inactive dependencies ([412a2fd](https://github.com/katesongsy/dort/commit/412a2fd352c685de73c5bde67912cb3a3be254f1))

### Maintenance

- Merge pull request #9 from katesongsy/feat/form-reference-pickers ([3e866f9](https://github.com/katesongsy/dort/commit/3e866f969d9126560481459c8f1fc08bfa12f86a))

- Merge pull request #8 from katesongsy/feat/pf1-help ([4ede805](https://github.com/katesongsy/dort/commit/4ede80526892bc94267b79fa2623f3e4a1b4dcd6))

### Tests

- **tui:** select the adoption preview action directly ([1f8c3c4](https://github.com/katesongsy/dort/commit/1f8c3c4427bd89b89fc2560802fcce8851bc4309))

- **tui:** follow visible tool review and help text ([8895d80](https://github.com/katesongsy/dort/commit/8895d800ee3a78d88ec2739313886a555636f639))

- **tui:** wait for the profile form after closing PF1 help ([0a1dd9b](https://github.com/katesongsy/dort/commit/0a1dd9ba85adf0df4cfe88678ae1a242f56368f2))
## v0.2.0 — 2026-11-24

### Documentation

- verify downloads with release checksums ([64812a7](https://github.com/katesongsy/dort/commit/64812a71985ba399f8b4fb66b958a103eb5a1b1a))

- **manifest:** explain v0.1 manifest upgrades and file previews ([98a670f](https://github.com/katesongsy/dort/commit/98a670f4faf8af443350144d84d90fba161cc9c6))

- **verify:** describe focused panes and deliberate approval ([921681c](https://github.com/katesongsy/dort/commit/921681c4d79046ba811d40d804d245ee993b4659))

- **design:** show the actual terminal reference offline ([c4c8825](https://github.com/katesongsy/dort/commit/c4c882560dc54e3bc6259abbf98c193389b9e9a1))

- **design:** add offline Dort TUI reference site ([4c37d3c](https://github.com/katesongsy/dort/commit/4c37d3cd1a9352e21aef872e38ad3b2d21ebb884))

- **install:** document the script and gate it in CI ([e30b5a9](https://github.com/katesongsy/dort/commit/e30b5a9a6fe3ed3bcaae4684bcd061ea5874ac12))

- clarify test runner and pin planner invariants model check ([c5c15fe](https://github.com/katesongsy/dort/commit/c5c15feb3b209935d18321c307ec814691834796))

- simplify guides and consolidate contributor documentation ([2fa5af2](https://github.com/katesongsy/dort/commit/2fa5af206bda4fc27ad3cf9d8017b3920fbe744b))

- streamline agent instructions ([68e3060](https://github.com/katesongsy/dort/commit/68e30605c64540ea9ffc48c1e404288eba2c6235))

- add Homebrew and mise installation channels ([94d6fcd](https://github.com/katesongsy/dort/commit/94d6fcd34fd17f692040ccfaa899c0eab66c55e1))

- document crates.io installation and publication ([81550f7](https://github.com/katesongsy/dort/commit/81550f7355ad4347a8f9ee361bc6d87110a914af))

### Features

- **tui:** preview managed content automatically and quiet directory rows ([ee5327b](https://github.com/katesongsy/dort/commit/ee5327ba69bbe67b1a786e8532a72c8068b779cf))

- **tui:** preview content, follow links and fold with arrows ([ec5b1cc](https://github.com/katesongsy/dort/commit/ec5b1cc6dad7735f1176faafdefc0066bca560f8))

- **tui:** focus ignored paths and session pickers ([da6c40f](https://github.com/katesongsy/dort/commit/da6c40fd4da5c19a44ecd928c7e87b4b8dd0330b))

- **tui:** focus declared, installed, and attempt workspaces ([4411045](https://github.com/katesongsy/dort/commit/4411045cece41ad5390295b3e21c28a3200214ea))

- **tui:** migrate Files and Git to shared workspace focus ([883557b](https://github.com/katesongsy/dort/commit/883557b3a88b7aa991f43e276422dabd4f49d040))

- **tui:** share split workspace focus and migrate Recovery ([1882ed7](https://github.com/katesongsy/dort/commit/1882ed715feaca55993c8c5f575203943e4319ec))

- **design-system:** export reference terminal captures ([2c5fc42](https://github.com/katesongsy/dort/commit/2c5fc42d6179792384578aaf9b84486d21ccb1fd))

- **design-system:** add standalone recovery reference terminal ([bc71fb2](https://github.com/katesongsy/dort/commit/bc71fb28926ac8f5124a17987a299b218b8db773))

- **tui:** preview the selected Git file patch ([22acfa1](https://github.com/katesongsy/dort/commit/22acfa1ae34884c7501d6200bbd8f9725871c522))

- **git:** read a patch for literal selected paths ([83a37ee](https://github.com/katesongsy/dort/commit/83a37ee76193a5ce02f5c18796b14759896e9abc))

- **tui:** give shared workbench chrome the Signal style ([0dec0a5](https://github.com/katesongsy/dort/commit/0dec0a535bd7dfb4933a1bd6038d8a87548c57a8))

- **tui:** give dialogs and split panes more room ([9cf6671](https://github.com/katesongsy/dort/commit/9cf6671bb7280b2c32ac320cc67af51bc936969e))

- **tui:** simplify file review actions ([671ea66](https://github.com/katesongsy/dort/commit/671ea66f3b3cac1b2b65ba27c952e9b36bd858b0))

- **install:** install dort from the release archive after a checksum check ([939956a](https://github.com/katesongsy/dort/commit/939956a812d842862cd7337c186f4499d1779b3f))

- **tools:** support Homebrew and npm on macOS, Linux and WSL ([189ee1e](https://github.com/katesongsy/dort/commit/189ee1e8b0043ca845394081c3fe1dbee621627e))

- **tui:** improve navigation and menu interactions ([4d6a4b0](https://github.com/katesongsy/dort/commit/4d6a4b073cf7b155a49ce266927f1f9c27fa94d7))

- **tui:** refine navigation and seed tool declarations from inventory ([7f01069](https://github.com/katesongsy/dort/commit/7f010691a5286605e686c7e1f566e8a37638e902))

- **tui:** migrate workbench to calm navigation ([578a7ca](https://github.com/katesongsy/dort/commit/578a7ca95a781c893d040fce081fc6191b04f501))

- **tui:** enforce shared design system and calm inventory ([bbc5109](https://github.com/katesongsy/dort/commit/bbc5109721752e8fda74daea0ae3c4162dfc7655))

- **tui:** unify workbench interactions and fresh-review outcomes ([2f8fac1](https://github.com/katesongsy/dort/commit/2f8fac1a306649c00098c0f7a2dff9057a2f662f))

- **tools:** install missing Ubuntu packages noninteractively ([c8f4999](https://github.com/katesongsy/dort/commit/c8f49999cdbb801c465ef2a7b1f75b69257160ca))

- **tools:** install missing Homebrew core formulae ([1ccf308](https://github.com/katesongsy/dort/commit/1ccf308260ac0acc82ea5392ab9d9edcf2d134b7))

- **tools:** install exact npm packages with guarded runtime scopes ([6fe32ab](https://github.com/katesongsy/dort/commit/6fe32ab9c10e235d55b8667e2a367bbc66968433))

- **tools:** install exact mise versions with guarded attempt closure ([a93e65c](https://github.com/katesongsy/dort/commit/a93e65c71865046f29e7119de6e1024d5dc38b60))

- **tools:** discover inspection defaults with one confirmation ([44ba6bc](https://github.com/katesongsy/dort/commit/44ba6bcc47eccc6984112d7fdb4220743624b8bb))

- **tools:** add manifest-only declarations and simple confirmations ([7c07143](https://github.com/katesongsy/dort/commit/7c07143f79cc06cb8c9bf66e4916c047097b72c4))

- **tools:** describe non-executable install commands and risks ([d61431a](https://github.com/katesongsy/dort/commit/d61431afb1dcf6a90383fcd1544528310d10bcfd))

- **tools:** reconcile explicit session snapshots with binary metadata ([6e17376](https://github.com/katesongsy/dort/commit/6e1737636397b56e4b23bf6e81507439061c0019))

- **tools:** classify package claims with pure dependency planning ([7c24826](https://github.com/katesongsy/dort/commit/7c24826adf67f865925758eb837e7385cea311a1))

- **tools:** expose approved inspection in CLI and TUI ([1253175](https://github.com/katesongsy/dort/commit/1253175935827cab63ada221baefb9e241b83df4))

- **tools:** add explicitly approved provider inspection core ([593f64e](https://github.com/katesongsy/dort/commit/593f64e05606a0a03f035f6f2cd600ff8cfd3c1f))

- prepare crates.io source packages ([3a941af](https://github.com/katesongsy/dort/commit/3a941af1ec58c5d46bfcf4372563ff7cf9297d8d))

### Fixes

- **tui:** keep reader facts and limits after the reorder ([c4a558f](https://github.com/katesongsy/dort/commit/c4a558f7acb2e6c99ec7dda65d9d37e652f74afa))

- **tui:** put reader tasks before provenance ([d1f1896](https://github.com/katesongsy/dort/commit/d1f18960b82868b08db81e1164da009bc2ebd987))

- **tui:** reject review evidence from a different viewport ([6786c74](https://github.com/katesongsy/dort/commit/6786c74f39de89d312c39f4332c0d49c4484c96f))

- **tui:** gate approval on the rendered effects tail ([2c31deb](https://github.com/katesongsy/dort/commit/2c31deb06cf5c6f0090e530420af2a9871e74327))

- **tui:** keep local navigation out of batch drafts ([a1e131f](https://github.com/katesongsy/dort/commit/a1e131ffb623dfbff38f7fc2208ba2d1c78d0c99))

- **tui:** keep workspace chrome and pointer focus consistent ([1e4a50d](https://github.com/katesongsy/dort/commit/1e4a50de8262eb8f9bbc0decc7fe4d2295b99551))

- **tui:** require the full report tail before approval ([32b08a4](https://github.com/katesongsy/dort/commit/32b08a48246275dc17f1be8b9a6d24ac88fa2b83))

- **tui:** require deliberate approval for guarded reviews ([374879d](https://github.com/katesongsy/dort/commit/374879d385a0f91b301d254b4636664f36a1c2a9))

- **tui:** keep reader wrapping and row targets aligned ([760d609](https://github.com/katesongsy/dort/commit/760d60975269766c5de5c387de313a3288e44107))

- **tui:** make operation reports easier to scan ([d2e60be](https://github.com/katesongsy/dort/commit/d2e60be0a46a8cbeb64baf3a248fa2cb21a71537))

- **tui:** make package manager choices discoverable ([cfd0a78](https://github.com/katesongsy/dort/commit/cfd0a78edd417ee629fa9f9c423356ca9d9151ea))

- **tui:** distinguish Help headings from body text ([1ff3fb8](https://github.com/katesongsy/dort/commit/1ff3fb87b926d166b4f5abb22bff46eee8db7082))

- **ci:** wait for the selected file in the unmanage test ([420f44d](https://github.com/katesongsy/dort/commit/420f44dee0674cc9f0e7fdaca2174dfa4787cddf))

- **tui:** return tool declarations to Installed ([2010030](https://github.com/katesongsy/dort/commit/20100304b93de45fb6a264238c6ebe3401b5c8cf))

- **ci:** include gzip in the installer fixture PATH ([84c55d8](https://github.com/katesongsy/dort/commit/84c55d889131f714a12a08413e978ead846c3f22))

- **tui:** keep review selection and details in view ([ab61fa9](https://github.com/katesongsy/dort/commit/ab61fa9d04bd3e72151c421aaa7db0b8bb5b90f8))

- **test:** resend a key the app refused during owned work ([0422755](https://github.com/katesongsy/dort/commit/04227552668e21b8611fbc601dbcb530409d7cba))

- **tools:** raise the install metadata bound and name its size ([8ab5c9f](https://github.com/katesongsy/dort/commit/8ab5c9f366e0c20b14ef565d287baa4b272804ff))

- **test:** order a lone Esc before the next PTY key ([332dc17](https://github.com/katesongsy/dort/commit/332dc173c93d415dcdd0aba8e26304fd9abb1245))

- **core:** return errors from the write paths instead of panicking ([54aa737](https://github.com/katesongsy/dort/commit/54aa73783ed0579b3a4db0ed32b3fb54b644a749))

### Maintenance

- Merge pull request #7 from katesongsy/refactor/tui-design-adoption ([00bbeb0](https://github.com/katesongsy/dort/commit/00bbeb0b41ab9b61d8b7effdfcb59f3d3bdfb728))

- Merge pull request #6 from katesongsy/feat/tui-design-standard ([9f6d21d](https://github.com/katesongsy/dort/commit/9f6d21d6ebe7a567c309521f960f17b25edb088d))

- **design:** verify the reference and offline captures ([73f5086](https://github.com/katesongsy/dort/commit/73f508661dc590d6f980f0bdfb19d0b785087474))

- Merge pull request #5 from katesongsy/fix/tui-report-hierarchy ([1818bc2](https://github.com/katesongsy/dort/commit/1818bc2375fd30ad84da9d020ca4e6fda3690785))

- add .audit/ to gitignore ([1bb5d3f](https://github.com/katesongsy/dort/commit/1bb5d3fc1bda5d3b7fb12f5b02a6bb8864f28f7c))

- Merge pull request #4 from katesongsy/feat/tui-choice-git-preview ([303c7ce](https://github.com/katesongsy/dort/commit/303c7ceeec57b198705d5d5a6ea3b0dec4963ec5))

- Merge pull request #3 from katesongsy/feat/tui-signal ([6972e3d](https://github.com/katesongsy/dort/commit/6972e3d20efcd8d4d0134ec169f4de2470077db5))

- Merge pull request #2 from katesongsy/feat/review-navigation ([ff6f1d4](https://github.com/katesongsy/dort/commit/ff6f1d464fa140d1297c5735c8b3c6f91f83b79c))

- Merge pull request #1 from katesongsy/refactor/agents-rules-findings ([19acb99](https://github.com/katesongsy/dort/commit/19acb99fd82bda73dc7962970163b00c5e1a8068))

- run the documented test runner instead of serial cargo test ([c0f27da](https://github.com/katesongsy/dort/commit/c0f27daf2db20014ea26b769626b5e598d986601))

- run branch work once, on the pull request ([b957e1c](https://github.com/katesongsy/dort/commit/b957e1cf5d439b508b7888445bb1463434031c01))

- **docs:** gate the writing rules and rename the changelog ([597c0c9](https://github.com/katesongsy/dort/commit/597c0c939e312d0a3a5fa669cfba4b831fa92841))

- **pi:** track verify-dort project skill ([4cb6bea](https://github.com/katesongsy/dort/commit/4cb6beab155284b6c0d572a91a2c16adcaa49941))

- ignore local pi agent directory ([2ddd369](https://github.com/katesongsy/dort/commit/2ddd36921abaded50fded1fb1c6be2a258683937))

- Add code principles to AGENTS.md ([60a13b0](https://github.com/katesongsy/dort/commit/60a13b0ad44a5818d45e7d402508f36979b784a7))

- Drop Ctrl-K Actions overlay and Ctrl-G menu ([e31fa09](https://github.com/katesongsy/dort/commit/e31fa0985a869d299928bc423343c57b76a67682))

- validate mise independently from Homebrew ([bbf8e0d](https://github.com/katesongsy/dort/commit/bbf8e0d6e21a6ca9e45740ec911e91bcb33f3117))

### Refactoring

- **tui:** open reviews on their full effects ([7bda638](https://github.com/katesongsy/dort/commit/7bda6381ea6c00e462a7ac2433a2363fbc8dc151))

- **tui:** remove unreachable confirmation dialog ([720d3bd](https://github.com/katesongsy/dort/commit/720d3bdf5c5933d24313c708a372143de96c8130))

- **tui:** share report text roles and pane focus ([51971c7](https://github.com/katesongsy/dort/commit/51971c756f0e64ac090a6242e020a78627258045))

- **tui:** share report readers and approval controls ([8abbe07](https://github.com/katesongsy/dort/commit/8abbe0746be0cebfc798b24d9f9b91a2d773847e))

- **tui:** share selectable collections and row targets ([51a3687](https://github.com/katesongsy/dort/commit/51a3687dbeb2023c83546cf63b4c384b3ca19dad))

- **tui:** give each feature its own result handling ([bfba7ac](https://github.com/katesongsy/dort/commit/bfba7ace78207cfe4a9bda79b28678b733e2832b))

- align UI copy and source notes with contributor rules ([df0fcdb](https://github.com/katesongsy/dort/commit/df0fcdbb9aa115d74a6ca4b48286e57e6a45ebf9))

### Tests

- **core:** previews show local sensitive-looking text ([f07d59e](https://github.com/katesongsy/dort/commit/f07d59e9a7c7161090f75acdc0e5cb91c7a2c39c))

- **tui:** drive the rendered tail before approval ([14b7c05](https://github.com/katesongsy/dort/commit/14b7c05b5d151c873a2ac6e214f4b74ec292ebde))

- **tui:** guard the single deliberate approval key ([c1f0e2c](https://github.com/katesongsy/dort/commit/c1f0e2c83cc02d9764500e1ee274d1d1a950c834))

- **tui:** prove the last effect is visible before approval ([7a8be0b](https://github.com/katesongsy/dort/commit/7a8be0bf5d36d01343bca9710d3d3301eec3268b))

- **tui:** check confirmation styling with the deliberate key ([564ae0e](https://github.com/katesongsy/dort/commit/564ae0e6ea260a2697699d571565ea21cf925367))

- **tui:** exercise deliberate approval journeys ([2e067ae](https://github.com/katesongsy/dort/commit/2e067ae051071bde5e06bda5c5a28a40e55f60dc))

- **tui:** guard shared collection and review boundaries ([ed8f940](https://github.com/katesongsy/dort/commit/ed8f940d6f9d688809d625cdfe7849fd6cd299be))

- **tui:** verify dropdowns and automatic Git previews ([160f5aa](https://github.com/katesongsy/dort/commit/160f5aa620d3aef67f762247fcc2093c3db3111d))

- **tui:** drive the new file review keys ([749c976](https://github.com/katesongsy/dort/commit/749c9769fb065c1d9e1fd020ef24e3e04b8780ff))

- **install:** drive install.sh against a loopback release fixture ([26a4edf](https://github.com/katesongsy/dort/commit/26a4edfbdd3b94d6409f3b351aa144d7659e4e47))

- **verify:** prove the disposable HOME path in CI ([0e43585](https://github.com/katesongsy/dort/commit/0e43585f91f1f278a30952ffbed7cddd1f72c7f9))

- serialize independent workspace test cases ([cfe38c7](https://github.com/katesongsy/dort/commit/cfe38c70d18bffea02f43130382ebd97db98c535))

- isolate bootstrap subprocess fixtures ([2f93d88](https://github.com/katesongsy/dort/commit/2f93d88b815c77d05417b525eae663fde244ae02))
## v0.1.0 — 2026-11-08

### Documentation

- align architecture with bounded storage coverage ([a0bf1cf](https://github.com/katesongsy/dort/commit/a0bf1cf56161d824be74f3b24232ad366b4e2f26))

- migrate end-user guides to docs/user and add validation gates ([cfaea64](https://github.com/katesongsy/dort/commit/cfaea6432c9875f129caba4e7f4f548d0e6d6fd6))

- Add initial README ([1f32976](https://github.com/katesongsy/dort/commit/1f3297636c7d64be7d7d13fb979e6ca6caab5348))

### Features

- **release:** prepare v0.1.0 native binary distribution ([8496939](https://github.com/katesongsy/dort/commit/84969390c323379fb06555fdc15655bf6f472b36))

- **tui:** separate menu navigation from contextual actions ([0d9e482](https://github.com/katesongsy/dort/commit/0d9e48225904690caeac61b3e45fd786fd1086bc))

- **tui:** add contextual actions and help with visible ignore controls ([0d98d8d](https://github.com/katesongsy/dort/commit/0d98d8dea4398ab1fb5699605f16d43f23893aea))

- **discover:** add persistent ignores and focused batch editing ([ec04936](https://github.com/katesongsy/dort/commit/ec0493618720ba92cc5e0746d7ad2873fca71e17))

- **tui:** support ASCII and monochrome terminals ([0e823be](https://github.com/katesongsy/dort/commit/0e823be7e385a8710b032ac50116ce64e283baaf))

- **tui:** add structured task activity and outcome panels ([c45f144](https://github.com/katesongsy/dort/commit/c45f144aae7e8c3d7664ea129f34b88c20a2a72b))

- **files:** add tree selection and guarded batch transactions ([186e2be](https://github.com/katesongsy/dort/commit/186e2be1e77f666a4ad1c8c17060d45b3ab73daf))

- **tui:** add direct keyboard and mouse navigation ([2ef224e](https://github.com/katesongsy/dort/commit/2ef224e738625cfd8c7bb879bc3b705bf520d3e5))

- **tui:** add contextual search-first action palette ([b6de53f](https://github.com/katesongsy/dort/commit/b6de53fde6bff4fe991bebaab26ac55a15cd6a7f))

- **tui:** add typed session pickers and reachable compact details ([bd80394](https://github.com/katesongsy/dort/commit/bd80394eaa822c1d095259bd58bca9900f8e9164))

- **tui:** add typed Git metadata tables and details ([2246363](https://github.com/katesongsy/dort/commit/2246363aacae5b85b3a2460e56c757726aee7592))

- **tui:** render typed recovery transactions and contextual actions ([b73f503](https://github.com/katesongsy/dort/commit/b73f50305f690d52b43736a47d905675272a04a7))

- **tui:** add typed overview and checks dashboards ([1334159](https://github.com/katesongsy/dort/commit/133415987b64672d13235f92480bcf7267fbcb07))

- **tui:** introduce styled workbench and structured file workflows ([b3d514b](https://github.com/katesongsy/dort/commit/b3d514be2c297ea2624651cf93a5b9c349a95ec4))

- **tui:** complete recovery and diagnostic workflows ([6b1aa91](https://github.com/katesongsy/dort/commit/6b1aa9196a86856c535699030560fe515024567f))

- **tui:** add read-only Git workflows and gated patch viewing ([ada7aa7](https://github.com/katesongsy/dort/commit/ada7aa7341d0847615b210c09447d5009b89163e))

- **tui:** complete local file workflows and manifest checks ([809ab5c](https://github.com/katesongsy/dort/commit/809ab5c17e1401fffa2d0fae164f410c9df14230))

- **tui:** add discoverable actions and guided repository onboarding ([30f8633](https://github.com/katesongsy/dort/commit/30f863358f9c228866533301172693f32db57ce2))

- **tui:** add guarded file and recovery workflows with responsive progress ([352b0a1](https://github.com/katesongsy/dort/commit/352b0a1cfbdf82af677afd725709b7ed0155900f))

- **cli:** add approved file lifecycle and recoverable repository bootstrap ([20ac569](https://github.com/katesongsy/dort/commit/20ac56918e73fb9e1efecf0ba5ff292dd336934d))

- **git:** add hardened read-only status and diff inspection ([bb1b897](https://github.com/katesongsy/dort/commit/bb1b897d5e2fbab5c9aaab56a952fe117f328d4a))

- **core:** add recoverable link apply and unmanage lifecycle ([5a2c015](https://github.com/katesongsy/dort/commit/5a2c015254935203e5e1520c4714e9d3615e81bf))

- **core:** add recoverable adoption and maintained project documentation ([e4df958](https://github.com/katesongsy/dort/commit/e4df958c71f0a6e45779bea398a4909ee0092c5c))

- Add secure read-only file observation and pure lifecycle plans ([a8052f4](https://github.com/katesongsy/dort/commit/a8052f413218e1eec9ad3e4cc909a7099a59e71b))

- Implement read-only foundation and isolated local state APIs ([a85f3ed](https://github.com/katesongsy/dort/commit/a85f3edf932a281eeefae1341313b2f0bb872b29))

### Fixes

- **release:** bundle dependency and Rust library notices ([09d3d4f](https://github.com/katesongsy/dort/commit/09d3d4fc5b1bac6d521c2adf01d7f26cadcfe5b1))

- **tui:** polish navigation, reviews and managed-file workflows ([065d364](https://github.com/katesongsy/dort/commit/065d3647b2cd62f3bdb9eaae9a6ca1725e94ff6c))

- **tui:** improve compact navigation and discovery eligibility ([57dc8d3](https://github.com/katesongsy/dort/commit/57dc8d3ea7429fee3ba5d54676a0b2aad0fab50c))

### Maintenance

- init ([bc0198e](https://github.com/katesongsy/dort/commit/bc0198eae64ec98af0aa2bcd8a7bb6a8da1ee5c8))

### Tests

- cover batch and inverse ENOSPC failures ([8a715d8](https://github.com/katesongsy/dort/commit/8a715d8647fd7679e04de1775dedbddb182dde3b))

- validate bounded macOS cross-device refusals ([5db5157](https://github.com/katesongsy/dort/commit/5db5157ecaeb7b9fc55be8458de19112171849bc))
