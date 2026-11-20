# Girens

Girens is a media player for [Plex](https://www.plex.tv/) and [Ister](https://github.com/ister-app) servers. You can watch and listen to your music, shows and movies with this GTK app, and to the audiobooks and podcasts of an Ister server, with accounts of both kinds signed in next to each other.

Both can be downloaded from for watching offline; what was watched that way is told to the server when it can be reached again.

## Signing in

A Plex account signs in on plex.tv in your browser, which also works with two-factor authentication. Girens shows a short code as well, to type at [plex.tv/link](https://plex.tv/link) on another device. A username and password can be entered in Girens itself, and a Plex server without a plex.tv account is reached by its address and a token. With a Plex Home you choose the profile to use.

An Ister server is signed in to by its address; the browser opens the page of its OpenID Connect provider. To stay signed in after the session in the browser has ended, Girens asks that provider for the `offline_access` scope. A provider that does not grant that to the `ister` client still works: Girens then asks to sign in again when the session has ended.

Titles and descriptions of an Ister server are shown in the language of your desktop. What the server does not have in that language, such as the title of an episode that was never translated, is shown in English.

## Why this app

Plex has a website and players of its own, and Ister has a web player. Girens adds what those lack on a Linux desktop or phone: a layout that adapts to a small screen, copies to watch without a connection, and an interface that fits in with GNOME (GTK 4 and libadwaita).

## How to install

Flatpak is the recommended installation method. You can get Girens Flatpak on Flathub.

<a href="https://flathub.org/apps/details/nl.g4d.Girens">
<img src="https://flathub.org/assets/badges/flathub-badge-i-en.png" width="190px" />
</a>

The Rust version is on the `beta` branch of Flathub:

```bash
flatpak remote-add --if-not-exists flathub-beta https://flathub.org/beta-repo/flathub-beta.flatpakrepo
flatpak install flathub-beta nl.g4d.Girens
```

## Password Storage

Girens never stores a password. The tokens it gets after signing in are kept in the
[Secret Service](https://www.freedesktop.org/wiki/Specifications/secret-storage-spec/),
so you should have something providing 
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

What goes wrong is written to the log as a warning; start Girens with
`G_MESSAGES_DEBUG=all` to see the rest too.

## Update dependencies

```bash
flatpak run org.flathub.flatpak-external-data-checker nl.g4d.Girens.yaml --edit-only
```

After changing Rust dependencies (`Cargo.lock`), regenerate the list of crates
the Flatpak build downloads:

```bash
python3 build-aux/cargo-sources.py
```
