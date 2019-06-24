from gi.repository import Gtk, GLib, GObject, Gdk, GdkPixbuf
from .gi_composites import GtkTemplate

from .cover_box import CoverBox
from .media_box import MediaBox

import threading

@GtkTemplate(ui='/nl/g4d/Girens/player_view.ui')
class PlayerView(Gtk.Box):
    __gtype_name__ = 'player_view'

    __gsignals__ = {
        'fullscreen': (GObject.SignalFlags.RUN_FIRST, None, (bool,)),
        'view-show-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,)),
        'view-album-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,)),
        'view-artist-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,))
    }

    _frame = GtkTemplate.Child()
    _overlay = GtkTemplate.Child()
    _stack = GtkTemplate.Child()
    _controlls = GtkTemplate.Child()
    _event = GtkTemplate.Child()
    _box = GtkTemplate.Child()
    _cover_image = GtkTemplate.Child()

    _deck_shows_box = GtkTemplate.Child()

    _download_key = None
    _download_thumb = None
    _item = None

    _timout = None
    _progress = 0
    _paused = True
    _fullscreen = False

    def __init__(self, window, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._event.add_events(Gdk.EventMask.POINTER_MOTION_MASK)
        self._event.connect("motion-notify-event", self.__on_motion)
        window.connect("key-press-event", self.__on_keypress)

    def set_player(self, player):
        self._player = player
        self._player.connect("media-playing", self.__on_media_playing)
        self._player.connect("playqueue-ended", self.__on_playqueue_ended)

    def set_plex(self, plex):
        self._plex = plex
        self._plex.connect("download-cover", self.__on_cover_downloaded)
        self._plex.connect("section-shows-deck", self.__on_show_deck_update)

        if (self._player != None):
            self._media_box = MediaBox(self._plex, self._player, fullscreen_button_show=True, show_only_type="video")
            self._controlls.add(self._media_box)
            self._media_box.set_visible(True)
            self._media_box.connect("fullscreen-clicked", self.__on_fullscreen_button_clicked)

    def __on_fullscreen_button_clicked(self, button):
        self.__fullscreen()

    def __fullscreen(self):
        self._fullscreen = not self._fullscreen
        self.emit('fullscreen', self._fullscreen)
        if self._fullscreen:
            self._box.hide()
            self._overlay.set_vexpand(True)
            self._overlay.set_size_request(-1, -1)
        else:
            self._box.show()
            self._overlay.set_vexpand(False)
            self._overlay.set_size_request(-1, 500)

    def __on_playqueue_ended(self, player):
        if self._fullscreen == True:
            self.__fullscreen()

    def __on_play_button_clicked(self, button):
        self._player.play_pause()


    def __on_keypress(self, widget, key):
        if key.keyval in [102, 65480]:
            self.__fullscreen()
        elif key.string == 'p':
            self._player.play_pause()
        elif key.string == 'o':
            self.__show_controlls()
        elif key.string == 'q':
            self._player.stop()

    def __on_motion(self, widget, motion):
        self.__show_controlls()

    def __show_controlls(self):
        self._controlls.show()
        if self._timout != None:
            GLib.source_remove(self._timout)
            self._timout = None

        self._timout = GLib.timeout_add(3000, self.__on_motion_over)

    def __on_motion_over(self):
        self._timout = None
        self._controlls.hide()

    def __on_media_playing(self, player, playing, item, playqueue, offset):
        self._playing = playing
        self._item = item

        if self._playing == True:
            GLib.idle_add(self.__empty_flowbox)
            thread = threading.Thread(target=self._plex.get_section_deck, args=(item.librarySectionID,))
            thread.daemon = True
            thread.start()

            if (not self._item.TYPE == 'playlist' and not self._item.TYPE == 'episode'):
                self._download_key = self._item.ratingKey
                self._download_thumb = self._item.thumb
            elif (self._item.type == 'playlist'):
                self._download_key = self._item.ratingKey
                self._download_thumb = self._item.composite
            else:
                self._download_key = self._item.grandparentRatingKey
                self._download_thumb = self._item.grandparentThumb

            thread = threading.Thread(target=self._plex.download_cover, args=(self._download_key, self._download_thumb))
            thread.daemon = True
            thread.start()


    def __on_cover_downloaded(self, plex, rating_key, path):
        if(self._download_key == rating_key):
            pix = GdkPixbuf.Pixbuf.new_from_file_at_size(path, 300, 300)
            GLib.idle_add(self.__set_image, pix)

    def __set_image(self, pix):
        self._cover_image.set_from_pixbuf(pix)

    def __empty_flowbox(self):
        for item in self._deck_shows_box.get_children():
            self._deck_shows_box.remove(item)

    def __on_show_deck_update(self, plex, items):
        for item in items:
            GLib.idle_add(self.__add_to_hub, self._deck_shows_box, item)

    def __add_to_hub(self, hub, item):
        cover = CoverBox(self._plex, item)
        cover.connect("view-show-wanted", self.__on_go_to_show_clicked)
        cover.connect("view-album-wanted", self.__on_go_to_album_clicked)
        cover.connect("view-artist-wanted", self.__on_go_to_artist_clicked)
        hub.add(cover)

    def __on_go_to_show_clicked(self, cover, key):
        self.emit('view-show-wanted', key)

    def __on_go_to_album_clicked(self, cover, key):
        self.emit('view-album-wanted', key)

    def __on_go_to_artist_clicked(self, cover, key):
        self.emit('view-artist-wanted', key)
