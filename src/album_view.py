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

from gi.repository import Gtk, GLib, GObject, Handy, GdkPixbuf
from .gi_composites import GtkTemplate
from .album_item import AlbumItem

import threading

@GtkTemplate(ui='/nl/g4d/Girens/album_view.ui')
class AlbumView(Handy.Column):
    __gtype_name__ = 'album_view'

    __gsignals__ = {
        'view-artist-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,)),
        'done-loading': (GObject.SignalFlags.RUN_FIRST, None, ())
    }

    _title_label = GtkTemplate.Child()
    _subtitle_label = GtkTemplate.Child()
    _item_box = GtkTemplate.Child()
    _cover_image = GtkTemplate.Child()

    _menu_button = GtkTemplate.Child()
    _play_button = GtkTemplate.Child()
    _artist_view_button = GtkTemplate.Child()
    _download_button = GtkTemplate.Child()
    _shuffle_button = GtkTemplate.Child()

    _download_key = None
    _key = None

    _timout = None

    def __init__(self, plex, artist_view=False, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex

        if artist_view == True:
            self._artist_view_button.set_visible(False)

        self._plex.connect("download-cover", self.__on_cover_downloaded)

        self._play_button.connect("clicked", self.__on_play_button_clicked)
        self._shuffle_button.connect("clicked", self.__on_shuffle_button_clicked)
        self._artist_view_button.connect("clicked", self.__on_go_to_artist_clicked)
        self._download_button.connect("clicked", self.__on_download_button)

    def change_album(self, key):
        self._title_label.set_text('')
        self._subtitle_label.set_text('')
        self._key = key
        for item in self._item_box.get_children():
            self._item_box.remove(item)

        self._connection_album_retrieved = self._plex.connect("album-retrieved", self.__album_retrieved)

        thread = threading.Thread(target=self._plex.get_album, args=(key,))
        thread.daemon = True
        thread.start()

    def __album_retrieved(self, plex, album, tracks):
        self._plex.disconnect(self._connection_album_retrieved)
        if self._key is not None and int(album.ratingKey) == int(self._key):
            GLib.idle_add(self.__album_process, album, tracks)

    def __album_process(self, album, tracks):
        self._item = album
        self._title_label.set_text(album.title)
        self._subtitle_label.set_text(str(album.year))

        self._download_key = album.ratingKey
        self._download_thumb = album.thumb

        thread = threading.Thread(target=self._plex.download_cover, args=(self._download_key, self._download_thumb))
        thread.daemon = True
        thread.start()

        self._tracks = tracks
        self.__start_add_items_timout()

    def __add_track(self, track):
        self._item_box.add(AlbumItem(self._plex, track))
        self._tracks.remove(track)

    def __show_more_items(self):
        self.__stop_add_items_timout()
        if len(self._tracks) > 0:
            i = 10
            while len(self._tracks) > 0:
                self.__add_track(self._tracks[0])
                i -= 1
                if (i == 0):
                    break
            if len(self._tracks) == 0:
                self.emit('done-loading')
            else:
                self.__start_add_items_timout()

    def __stop_add_items_timout(self):
        if self._timout != None:
            GLib.source_remove(self._timout)
            self._timout = None

    def __start_add_items_timout(self):
        if len(self._tracks) > 0:
            self._timout = GLib.timeout_add(100, self.__show_more_items)

    def __on_cover_downloaded(self, plex, rating_key, path):
        if(self._download_key == rating_key):
            pix = GdkPixbuf.Pixbuf.new_from_file_at_size(path, 150, 150)
            GLib.idle_add(self.__set_image, pix)

    def __set_image(self, pix):
        self._cover_image.set_from_pixbuf(pix)

    def __on_go_to_artist_clicked(self, button):
        self.emit('view-artist-wanted', self._item.parentRatingKey)

    def __on_play_button_clicked(self, button):
        thread = threading.Thread(target=self._plex.play_item, args=(self._item,))
        thread.daemon = True
        thread.start()

    def __on_download_button(self, button):
        self._menu_button.set_active(False)
        if (self._item.TYPE == 'show'):
            self._sync_settings = SyncSettings(self._plex, self._item)
            self._sync_settings.show()
        else:
            thread = threading.Thread(target=self._plex.add_to_sync, args=(self._item,))
            thread.daemon = True
            thread.start()

    def __on_shuffle_button_clicked(self, button):
        self._menu_button.set_active(False)
        thread = threading.Thread(target=self._plex.play_item, args=(self._item,),kwargs={'shuffle':1})
        thread.daemon = True
        thread.start()
