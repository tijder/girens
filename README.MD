# Girens for Plex

Girens is a [Plex](https://www.plex.tv/) media player client. You can watch and listen to your music, shows and movies with this GTK app.

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

The easiest way to build this App from source is with [Gnome-builder](https://wiki.gnome.org/Apps/Builder). Install Gnome-builder on your machine. Clone thise git project to your machine. Open the git project in Gnome-builder and click run.

## Screenshot

![Screenshot of Girens with covers blurred](https://g4d.nl/girens/screenshot_films_blurred.png)
