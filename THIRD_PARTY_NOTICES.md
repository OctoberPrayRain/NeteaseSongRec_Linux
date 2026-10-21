# Third-party notices

## SongRec

The audio capture and audio device backends are adapted from
[SongRec](https://github.com/marin-m/SongRec), by marin-m and contributors.
The source used for this project identifies itself as version 0.7.6,
commit `14b14e313dc162d63805f574d7164cc1c47b4a08`.

The adapted files are `src/core/microphone_thread.rs` and the files in
`src/core/audio_controllers/`. They have been modified for six-second audio
segments, NetEase recognition sessions, and system playback device selection.
SongRec is licensed under GPL-3.0-or-later. Its GPL text is included in
`LICENSES/GPL-3.0.txt`. These upstream portions retain their original licensing;
the repository's AGPL license does not replace the upstream license.

## Slint and other Rust dependencies

The UI uses Slint 1.14.1 under its GPL-3.0-only licensing option. Other Rust
dependencies retain their respective upstream licenses. `Cargo.lock` records
the versions used to build the application; their license texts and notices
are provided by the corresponding upstream packages.

## NetEase Music extension resources

The fingerprint engine `afp.wasm`, its `sandbox.bundle.js` glue, and the
extension's icons originate from NetEase's 云音乐听歌识曲 extension, version 1.0.4.
Their official sources are the
[extension download page](https://fn.music.163.com/g/chrome-extension-home-page-beta/)
and [Chrome Web Store listing](https://chromewebstore.google.com/detail/kemcalcncfhmdkgglekijclbomdoohkp).

These resources and NetEase's names and marks belong to their respective rights
holders. They are not covered by this repository's AGPL license. No separate
redistribution license was found in the inspected extension archive; public
availability of that archive is not represented here as a grant of additional
rights. The application's Node.js bridge is independently maintained in this
repository and adapts the extension glue at runtime.

NeteaseSongRec_Linux is a community project. It is not an official NetEase
product and has no claimed affiliation, partnership, or endorsement from NetEase
or the SongRec maintainers.
