# Third-party notices

BootPilot's original source is MIT licensed; see LICENSE. Dependencies retain
their respective licenses. The packaged THIRD-PARTY-LICENSES.txt is generated
from the locked Rust packages and production npm dependencies and carries their
published license/notice files, including build-time Rust dependencies for
conservative coverage. Their presence does not imply endorsement.

Principal components include [Tauri](https://github.com/tauri-apps/tauri),
[React](https://github.com/facebook/react),
[i18next](https://github.com/i18next/i18next) and
[react-i18next](https://github.com/i18next/react-i18next),
[rmcp](https://github.com/modelcontextprotocol/rust-sdk),
[Tokio](https://github.com/tokio-rs/tokio), and
[windows-rs](https://github.com/microsoft/windows-rs).

Microsoft Edge WebView2 is a separately licensed Microsoft runtime. The NSIS
installer detects its presence and, if necessary, obtains Microsoft's runtime
bootstrapper. It is not a bootloader. NSIS and its bundled installer components
retain their own upstream notices in the generated installer.

BootPilot does not bundle Grub2Win, GRUB, CachyOS, Microsoft Windows bootloaders,
copied bootloader source, or third-party bootloader artwork.
