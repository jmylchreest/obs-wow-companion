# Your first recording

OBS WoW Companion uses OBS to record encounters and keeps them in a library for review. You keep your existing OBS scenes and sound settings.

This is a development preview. Try a short recording before using it for a whole raid or evening of keys. Linux is the main development platform; Windows builds need more real gameplay testing. macOS is not supported yet.

## 1. Download and unpack the app

Use the files attached to a tagged [GitHub prerelease](https://github.com/jmylchreest/obs-wow-companion/releases). Choose the Linux x86-64 archive or Windows x86-64 ZIP, extract the whole folder, then run `obs-wow-companion` or `obs-wow-companion.exe`. If no prerelease is listed yet, the preview is still being prepared; the [source build instructions](../README.md#building-from-source) are available for developers.

Keep the included documentation and OBS templates with the app. You do not need Rust to run a packaged build.

You also need:

- **OBS Studio**, already able to record your WoW window and the sound you want.
- **FFmpeg and ffprobe**, installed separately. If Companion cannot find them, choose their executable paths under **Settings → Advanced**.
- On Linux, **GTK 3 and WebKitGTK 4.1** runtime libraries. The Linux package is built on Ubuntu 24.04; other distributions still need testing.
- On Windows, **Microsoft Edge WebView2 Runtime**.

On Ubuntu 24.04, the runtime packages are:

```sh
sudo apt install libgtk-3-0t64 libwebkit2gtk-4.1-0 ffmpeg gstreamer1.0-libav gstreamer1.0-plugins-good
```

See the [compatibility table](compatibility.md) for tested combinations. Packages are not yet code-signed; follow your operating system's normal checks before opening a downloaded app. Release files include SHA-256 checksums.

## 2. Connect OBS

Open OBS. Under **Tools → WebSocket Server Settings**, enable the server and authentication. Leave the port at **4455** unless you already use another one.

Open Companion and follow setup. Enter the port and password shown in OBS. The password is saved in your operating system's password store when that service is available.

If the pill says **OBS offline**, check that OBS and its WebSocket server are running. If it says **Password store**, unlock your password store or enter the password again for this session. Companion retries automatically; the pill shows the current reason and next retry.

## 3. Check recording settings

Let Companion set its temporary recording folder. In OBS, choose **Settings → Output → Advanced → Recording → Standard** and enable **Automatic File Splitting → Only split manually**. This lets completed clips become available while you keep playing.

For a straightforward first setup, use MKV, H.264 video and AAC audio. Keep your working scene and sound sources. The [OBS guide](obs-setup.md) covers scene switching, platform capture choices and multiple audio tracks. An [optional starter profile](../templates/obs/README.md) is included if you are starting from scratch.

Record a short clip in OBS and play it back to check the picture, game sound and microphone. Companion cannot tell whether a capture source is showing the right window.

## 4. Choose WoW and turn on logging

Select your WoW installation in setup. You can add more than one. If it is not found, browse to the game's **Logs** folder, including the Wine or Proton installation if you play on Linux.

In WoW, enter `/combatlog`, or use an addon that enables logging for you. Leave OBS and Companion open with **Auto recording on**. The next supported encounter starts recording automatically. Under **Settings → WoW installations → Automatic recording**, you can turn raids or keys off, set minimum difficulty or key level, and opt into experimental delve recording.

If you started recording in OBS yourself, Companion leaves that recording under your control. Some clips may wait until you stop it in OBS.

## 5. Watch it back

Select a recording. Click a death to jump five seconds before it, or use the timeline markers to find a boss fight. Space plays or pauses; the arrow keys seek. Use the speed and audio controls in the player.

To share a clip, expand **Export**, choose a range and audio tracks, then save the ZIP. It contains the video and a small player with the markers. See [sharing with your group](sharing.md).

## If something goes wrong

Open **Events** using the small arrow at the bottom right. For missing clips, use **Settings → Advanced → Check recordings**. Keep original source files until the clip is ready or you have decided it is no longer needed.

An **Unrecoverable** recording has no usable matching footage in the checked folder. **Attention needed** means there is still something to review or retry. Files awaiting review are not silently deleted.

[Report a problem](https://github.com/jmylchreest/obs-wow-companion/issues/new/choose) with the app version, your setup and the relevant events. Remove passwords, private session links, personal paths and character names from anything you attach.
