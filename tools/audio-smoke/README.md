# audio-smoke

This Windows-only test opens `CABLE Input (VB-Audio Virtual Cable)` for render
and `CABLE Output (VB-Audio Virtual Cable)` for capture, emits a 997 Hz tone for
three seconds, and checks frame count, RMS, tone amplitude, and clipping. It
tests VB-CABLE without involving a physical microphone or the GUI.

`just build-windows` cross-builds the standalone test to
`out/NoiseHoiHoi-v0.4-audio-smoke.exe`. Run it on Windows 11 x64 after
installing VB-CABLE and restarting Windows:

```powershell
.\NoiseHoiHoi-v0.4-audio-smoke.exe
```

Use `--seconds N` to measure from 1 to 30 seconds. A successful run prints
`PASS` with the captured frame count, RMS, peak, and measured 997 Hz amplitude.
