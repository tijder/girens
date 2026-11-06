# Girens

Girens is a media player for [Plex](https://www.plex.tv/) and [Ister](https://github.com/ister-app) servers. You can watch and listen to your music, shows and movies with this GTK app, with accounts of both kinds signed in next to each other.

Downloads for watching offline are for Plex only, as an Ister server only streams.

## Why this app

There are more Plex clients that can be used on Linux. For example the Plex website and the [Plex media Player](https://www.plex.tv/apps-devices/#modal-devices-plex-media-player). Those Plex clients are lacking functions which I like to have. For example: a responsive layout (so you can use it on a small screen), function to download media items to the device and GTK+ is awesome.

## How to install

Flatpak is the recommended installation method. You can get Girens Flatpak on Flathub.

<a href="https://flathub.org/apps/details/nl.g4d.Girens">
<img src="https://flathub.org/assets/badges/flathub-badge-i-en.png" width="190px" />
</a>

Or download the latest development [flatpak](https://gitlab.gnome.org/tijder/girens/-/jobs/artifacts/master/raw/nl.g4d.Girens.flatpak?job=flatpak) from gitlab. Then install that flatpak with:
```bash
flatpak install nl.g4d.girens.flatpak
```

## Password Storage

Girens uses [Secret Service](https://www.freedesktop.org/wiki/Specifications/secret-storage-spec/)
to store the password so you should have something providing 
that service on your system. If you're using GNOME or KDE
this should work for you out of the box with gnome-keyring or
ksecretservice.

## How to build

```bash
flatpak-builder --user --force-clean --install-deps-from=flathub builddir nl.g4d.Girens.yaml
flatpak-builder --run builddir nl.g4d.Girens.yaml girens
```

### Working on Girens

Girens is written in Rust. The commands above build your checkout from
scratch. While developing it is quicker to build the dependencies once and
then the app itself inside the build sandbox:

```bash
flatpak-builder --user --force-clean --install-deps-from=flathub --stop-at=girens builddir-rust nl.g4d.Girens.yaml
flatpak build --share=network --filesystem=$PWD --env=PATH=/app/bin:/usr/bin:/usr/lib/sdk/rust-stable/bin builddir-rust \
  sh -c "cd $PWD && meson setup builddir-rust-meson --prefix=/app --libdir=lib -Dbuildtype=debug -Dnetwork_tests=false && ninja -C builddir-rust-meson install"
flatpak build --socket=wayland --socket=fallback-x11 --share=ipc --share=network --device=dri --socket=pulseaudio builddir-rust girens
```

`meson test -C builddir-rust-meson` (run the same way) runs the Rust tests
and Clippy.

## Update depedencies

```bash
flatpak run org.flathub.flatpak-external-data-checker nl.g4d.Girens.yaml --edit-only
```

After changing Rust dependencies (`Cargo.lock`), regenerate the list of crates
the Flatpak build downloads:

```bash
python3 build-aux/cargo-sources.py
```
