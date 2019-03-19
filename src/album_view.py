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

    _title_label = GtkTemplate.Child()
    _subtitle_label = GtkTemplate.Child()
    _item_box = GtkTemplate.Child()
    _cover_image = GtkTemplate.Child()

    _download_key = None
    _key = None

    def __init__(self, plex, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex

        self._plex.connect("album-retrieved", self.__album_retrieved)
        self._plex.connect("download-cover", self.__on_cover_downloaded)

    def change_album(self, key):
        self._title_label.set_text('')
        self._subtitle_label.set_text('')
        self._key = key
        for item in self._item_box.get_children():
            self._item_box.remove(item)

        thread = threading.Thread(target=self._plex.get_album, args=(key,))
        thread.daemon = True
        thread.start()

    def __album_retrieved(self, plex, album, tracks):
        if int(album.ratingKey) == int(self._key):
            GLib.idle_add(self.__album_process, album, tracks)

    def __album_process(self, album, tracks):
        self._title_label.set_text(album.title)
        self._subtitle_label.set_text(str(album.year))

        self._download_key = album.ratingKey
        self._download_thumb = album.thumb

        thread = threading.Thread(target=self._plex.download_cover, args=(self._download_key, self._download_thumb))
        thread.daemon = True
        thread.start()

        i = 0
        for track in tracks:
            self._item_box.add(AlbumItem(self._plex, track, i))
            i+= 1

    def __on_cover_downloaded(self, plex, rating_key, path):
        if(self._download_key == rating_key):
            pix = GdkPixbuf.Pixbuf.new_from_file_at_size(path, 150, 150)
            GLib.idle_add(self.__set_image, pix)

    def __set_image(self, pix):
        self._cover_image.set_from_pixbuf(pix)
