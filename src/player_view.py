from gi.repository import Gtk, GLib, GObject, Gdk, GdkPixbuf
from .gi_composites import GtkTemplate

import threading

@GtkTemplate(ui='/nl/g4d/Girens/player_view.ui')
class PlayerView(Gtk.Box):
    __gtype_name__ = 'player_view'

    __gsignals__ = {
        'fullscreen': (GObject.SignalFlags.RUN_FIRST, None, (bool,)),
    }

    _frame = GtkTemplate.Child()
    _overlay = GtkTemplate.Child()
    _stack = GtkTemplate.Child()
    _controlls = GtkTemplate.Child()
    _event = GtkTemplate.Child()
    _play_button = GtkTemplate.Child()
    _fullscreen_button = GtkTemplate.Child()
    _play_image = GtkTemplate.Child()
    _progress_bar = GtkTemplate.Child()
    _cover_image = GtkTemplate.Child()

    _box = GtkTemplate.Child()

    _download_key = None
    _download_thumb = None

    _timout = None
    _progress = 0
    _paused = True
    _fullscreen = False

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._event.add_events(Gdk.EventMask.POINTER_MOTION_MASK)
        self._event.connect("motion-notify-event", self.__on_motion)
        self._play_button.connect("clicked", self.__on_play_button_clicked)
        self._fullscreen_button.connect("clicked", self.__on_fullscreen_button_clicked)

    def set_player(self, player):
        self._player = player
        self._player.connect("media-playing", self.__on_media_playing)
        self._player.connect("media-paused", self.__on_media_paused)
        self._player.connect("media-time", self.__on_media_time)

    def set_plex(self, plex):
        self._plex = plex
        self._plex.connect("download-cover", self.__on_cover_downloaded)

    def __on_fullscreen_button_clicked(self, button):
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

    def __on_play_button_clicked(self, button):
        self._player.play_pause()

    def __on_motion(self, widget, motion):
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
        self._offset = offset

        GLib.idle_add(self.__update_buttons)

        if self._playing == True:
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

    def __on_media_paused(self, player, paused):
        self._paused = paused
        GLib.idle_add(self.__update_buttons)

    def __on_media_time(self, player, time):
        self._progress = time
        GLib.idle_add(self.__update_buttons)

    def __update_buttons(self):
        if (self._paused == True):
            self._play_image.set_from_icon_name('media-playback-start-symbolic', 4)
        else:
            self._play_image.set_from_icon_name('media-playback-pause-symbolic', 4)

        if (self._item != None):
            self._progress_bar.set_fraction(self._progress / self._item.duration)

    def __on_cover_downloaded(self, plex, rating_key, path):
        if(self._download_key == rating_key):
            pix = GdkPixbuf.Pixbuf.new_from_file_at_size(path, 300, 300)
            GLib.idle_add(self.__set_image, pix)

    def __set_image(self, pix):
        self._cover_image.set_from_pixbuf(pix)
