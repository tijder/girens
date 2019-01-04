# window.py
#
# Copyright 2018 Gerben Droogers
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <http://www.gnu.org/licenses/>.

from gi.repository import Gtk, GLib, GObject, GdkPixbuf, Gdk
from .gi_composites import GtkTemplate
from .cover_box import CoverBox
from .playqueue_popover import PlayqueuePopover

import threading

@GtkTemplate(ui='/nl/g4d/Girens/media_box.ui')
class MediaBox(Gtk.Revealer):
    __gtype_name__ = 'media_box'

    _close_button = GtkTemplate.Child()
    _play_button = GtkTemplate.Child()
    _prev_button = GtkTemplate.Child()
    _next_button = GtkTemplate.Child()
    _play_image = GtkTemplate.Child()

    _title_label = GtkTemplate.Child()
    _subtitle_label = GtkTemplate.Child()
    _progress_bar = GtkTemplate.Child()

    _playqueue_button = GtkTemplate.Child()
    _cover_image = GtkTemplate.Child()

    _item = None
    _paused = False
    _playing = False
    _progress = 0

    _download_key = None
    _download_thumb = None

    def __init__(self, plex, player, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex
        self._player = player

        self._playqueue_popover = PlayqueuePopover(self._plex, self._player)
        self._playqueue_button.set_popover(self._playqueue_popover)

        self.__update_buttons()

        self._plex.connect("download-cover", self.__on_cover_downloaded)
        self._player.connect("media-paused", self.__on_media_paused)
        self._player.connect("media-playing", self.__on_media_playing)
        self._player.connect("media-time", self.__on_media_time)
        self._playqueue_popover.connect("show-button", self.__on_playqueue_show_button)
        self._play_button.connect("clicked", self.__on_play_button_clicked)
        self._prev_button.connect("clicked", self.__on_prev_button_clicked)
        self._next_button.connect("clicked", self.__on_next_button_clicked)
        self._close_button.connect("clicked", self.__on_close_button_clicked)

    def __on_play_button_clicked(self, button):
        if (self._paused == True):
            self._player.play()
        else:
            self._player.pause()

    def __on_close_button_clicked(self, button):
        self._player.stop()

    def __on_prev_button_clicked(self, button):
        thread = threading.Thread(target=self._player.prev)
        thread.daemon = True
        thread.start()

    def __on_next_button_clicked(self, button):
        thread = threading.Thread(target=self._player.next)
        thread.daemon = True
        thread.start()

    def __on_media_paused(self, player, paused):
        self._paused = paused
        GLib.idle_add(self.__update_buttons)

    def __on_media_playing(self, player, playing, item, playqueue, offset):
        self._playing = playing
        self._item = item
        self._playqueue = playqueue
        self._offset = offset

        if (playing == True):
            self.__reload_image()

        GLib.idle_add(self.__update_buttons)

    def __reload_image(self):
        if (not self._item.TYPE == 'playlist'):
            self._download_key = self._item.ratingKey
            self._download_thumb = self._item.thumb
        elif (self._item.type == 'playlist'):
            self._download_key = self._item.ratingKey
            self._download_thumb = self._item.composite

        thread = threading.Thread(target=self._plex.download_cover, args=(self._download_key, self._download_thumb))
        thread.daemon = True
        thread.start()

    def __on_media_time(self, player, time):
        self._progress = time
        GLib.idle_add(self.__update_buttons)

    def __update_buttons(self):
        if (self._paused == True):
            self._play_image.set_from_icon_name('media-playback-start-symbolic', 4)
        else:
            self._play_image.set_from_icon_name('media-playback-pause-symbolic', 4)

        if (self._item != None):
            self._prev_button.set_sensitive(self._offset - 1 >= 0)
            self._next_button.set_sensitive(self._offset + 1 < len(self._playqueue.items))
            self._progress_bar.set_fraction(self._progress / self._item.duration)

            title = ''
            subtitle = ''

            if (self._item.TYPE == 'episode'):
                title = self._item.grandparentTitle
                subtitle = self._item.seasonEpisode + ' - ' + self._item.title
            elif (self._item.TYPE == 'movie'):
                title = self._item.title
                subtitle = str(self._item.year)
            elif (self._item.TYPE == 'track'):
                title = self._item.title
                subtitle = self._item.grandparentTitle + ' - ' + self._item.parentTitle

            self._title_label.set_text(title)
            self._subtitle_label.set_text(subtitle)

        self.set_reveal_child(self._playing)

    def __on_playqueue_show_button(self, playqueue):
        self._playqueue_button.set_active(False)

    def __on_cover_downloaded(self, plex, rating_key, path):
        if(self._download_key == rating_key):
            pix = GdkPixbuf.Pixbuf.new_from_file_at_size(path, 50, 50)
            GLib.idle_add(self.__set_image, pix)

    def __set_image(self, pix):
        self._cover_image.set_from_pixbuf(pix)
