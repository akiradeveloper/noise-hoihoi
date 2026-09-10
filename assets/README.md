# Project icon

`NoiseHoiHoi.svg` is the shared source for the project icon. The root README
and Linux AppImage use it directly. The generated `NoiseHoiHoi.png` (256 × 256)
is embedded in the GUI for the main window and signal monitor on Windows and
Linux, including the 16-pixel icon beside each window's title.
`NoiseHoiHoi.ico` contains 16, 24, 32, 48, 64, 128, and 256 pixel images
and is embedded in the Windows application, installer, and uninstaller.
Windows shortcuts and the installed-app entry use the executable's icon.

After editing the SVG, regenerate and commit both derived files. On Debian or
Ubuntu, install the conversion tools once, then run from the repository root:

```sh
sudo apt-get install python3-gi-cairo python3-pil gir1.2-rsvg-2.0
python3 scripts/generate-icons.py
```

Normal application builds use the committed assets and do not need these
image-generation tools. Windows builds also compile the ICO as an executable
resource through `winresource`; the Windows Docker image already supplies the
MinGW resource compiler.
