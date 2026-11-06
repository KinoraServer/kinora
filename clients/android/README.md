# Kinora for Android TV

A native Android TV app for your Kinora server: browse your libraries and play movies and shows with direct play whenever the TV can handle the file (including HEVC, HDR10, HLG, Dolby Vision, PGS subtitles and Dolby/DTS passthrough), and the server's transcoding when it can't.

Built with Kotlin, Jetpack Compose for TV and AndroidX Media3. Tested target: NVIDIA Shield TV.

## What works

- Connect to a server by address (LAN IP, Tailscale name or a reverse-proxy URL, including a sub-path) and sign in.
- Home rows (continue watching, next up, recently added), libraries with sorting, movie, series, season and episode pages.
- Playback: direct play of MP4/MKV/WebM/TS, HLS when the server remuxes or transcodes, resume, progress saved every 10 seconds, mark watched.
- Audio and subtitle menus. Embedded subtitles (SRT, ASS, PGS, VobSub) during direct play; the server's WebVTT tracks during HLS.
- **Info** panel in the player that shows the method (direct play / remux / transcode), the server's reasons, and what the decoder actually receives (resolution, codec, HDR transfer, frame rate). Use it to check Dolby Vision.
- **Settings** shows what the TV reports it can play, e.g. `HEVC · HDR10 · Dolby Vision · TrueHD · up to 2160p`.
- Your server's default theme colours.

Not yet: quick-connect sign-in, Watch Next on the home screen, frame-rate matching, the server's player style, downloads, casting, the phone app. See the Android plan for the order.

## Install on your Shield (sideload)

### 1. Get the APK

- **Latest development build**: open the repo's **Actions** tab → **Android TV** → the newest green run → download the `kinora-tv-dev-…` artifact (a zip containing the APK).
- **Releases**: once release signing is set up (below), each `android-v*` tag publishes `kinora-tv-<version>.apk` on the **Releases** page.

The repo is private, so the TV can't download from GitHub directly. Download the APK on your Mac and send it to the TV with one of these:

### 2a. adb over Wi-Fi (recommended)

1. On the Shield: **Settings → Device Preferences → About**, press OK on **Build** seven times to unlock developer options.
2. **Settings → Device Preferences → Developer options**: turn on **Network debugging** and note the IP address it shows.
3. On the Mac: `brew install android-platform-tools`
4. `adb connect <shield-ip>:5555` and press **Allow** on the TV.
5. `adb install -r ~/Downloads/app-tv-release.apk`

Kinora appears in the Shield's app row. `-r` replaces an existing install and keeps your sign-in.

### 2b. Without a computer cable or adb

Install **Send Files to TV** (or similar) on the Shield and your phone, send the APK, then open it with a file manager on the Shield. You need to allow that file manager to install unknown apps (**Settings → Device Preferences → Security & restrictions → Unknown sources**).

### 3. First run

Enter your server's address, e.g. `192.168.1.20:8420` (the port defaults to 8420 if you leave it out) or `https://media.example.com`. Then sign in with your Kinora username and password. Plain `http://` works on your home network or Tailscale.

### Updating

Install the newer APK the same way. Development builds and release builds are signed with different keys, so switching between them needs an uninstall first (you'll sign in again).

## Release signing

Development builds are signed with `signing/dev.keystore`, which is committed on purpose (password `kinora-dev`) so every dev build installs over the last one. It is **not secret** and must not be used for anything you publish.

For real releases, create your own key once and keep a backup somewhere safe. Losing it means you can't update an installed app without uninstalling it, and the same key is needed if you ever move to the Play Store.

```sh
keytool -genkeypair -keystore kinora-release.keystore -storetype PKCS12 \
  -alias kinora -keyalg RSA -keysize 4096 -validity 10000 \
  -dname "CN=Kinora"
base64 -i kinora-release.keystore | pbcopy   # macOS: copies it for the secret below
```

Then in GitHub: **Settings → Secrets and variables → Actions → New repository secret**, add:

| Secret                     | Value                                                       |
| -------------------------- | ----------------------------------------------------------- |
| `KINORA_KEYSTORE_BASE64`   | the base64 text you copied                                  |
| `KINORA_KEYSTORE_PASSWORD` | the keystore password you chose                             |
| `KINORA_KEY_ALIAS`         | `kinora`                                                    |
| `KINORA_KEY_PASSWORD`      | the key password (same as the keystore password for PKCS12) |

Release: `git tag android-v0.1.0 && git push origin android-v0.1.0`.

## Developing

Open `clients/android` in Android Studio (it runs Android TV emulators on Apple Silicon). Or from a terminal with the Android SDK installed:

```sh
cd clients/android
./gradlew test                      # JVM unit tests for core/model and core/api
./gradlew :app-tv:installDebug      # build and install on a connected TV or emulator
```

### Layout

```
core/model     plain Kotlin: API types, capability rules, theme colour parsing, formatting
core/api       plain Kotlin: typed HTTP client for the Kinora API (OkHttp + kotlinx.serialization)
core/playback  Android: reads the device's decoders, display HDR and HDMI audio; builds the ExoPlayer
app-tv         the TV app (Compose for TV)
```

Logic that can be pure lives in `core/model` and is unit-tested (`CapsTest` has profiles for a Shield on a Dolby Vision TV, a Shield on an SDR TV and a phone). The API client is hand-written to match `web/src/lib/types.ts` until the server publishes an OpenAPI spec, at which point it will be generated.
