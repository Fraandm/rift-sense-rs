# Third-party notices

RiftSense uses data, assets and open-source software from third parties.

## Riot Games / League of Legends

- RiftSense is not affiliated with, endorsed by or sponsored by Riot Games, Inc.
- League of Legends, Data Dragon, champion icons, item icons, spell icons and
  related game assets are trademarks or registered trademarks of Riot Games,
  Inc.
- Game data is read through Riot's local Live Client Data API and the League
  Client (LCU). RiftSense does not read game memory, inject into the game,
  automate purchases, or send gameplay inputs.
- Riot's third-party policies and terms apply to your use of League of Legends.

## Leaguepedia

- Historical professional-match data used by the dataset was obtained from
  Leaguepedia / lol.fandom.com through its Cargo API.
- Leaguepedia content is attributed to its contributors and is subject to the
  licensing terms published by Fandom/Leaguepedia.
- RiftSense does not claim ownership of the underlying match data.

## Machine-learning models

- The bundled ONNX models were trained from the processed dataset using the
  Python pipeline in the original project.
- ONNX Runtime/format references belong to their respective owners. Inference
  in RiftSense uses `tract-onnx`.

## Open-source Rust crates

RiftSense is built with open-source crates including Axum, Tokio, Tower,
Serde, Reqwest, Rustls, Tract, Wry, Tao, Tray Icon, Muda, Rust Embed,
Tracing, Chrono, Clap, Winreg and Windows Sys. Their licenses are available in
their crate repositories and in `Cargo.lock`. They are not part of the
proprietary RiftSense source.

## Microsoft WebView2

The desktop application uses the Microsoft Edge WebView2 Runtime. WebView2 is
distributed under Microsoft's terms. The installer does not bundle the runtime
unless explicitly stated.

## Inno Setup

Windows installers are built with Inno Setup. Inno Setup is distributed under
its own license; see <https://jrsoftware.org/isinfo.php>.

## Fonts

The interface loads Chakra Petch and Inter from Google Fonts. Those fonts are
distributed under the SIL Open Font License by their respective authors.

## Reporting

If you believe an attribution or license is missing, open an issue in the
public repository before redistributing the project.
