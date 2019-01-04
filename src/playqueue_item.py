# cover_box.py
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

import cairo
import threading

@GtkTemplate(ui='/nl/g4d/Girens/playqueue_item.ui')
class PlayqueueItem(Gtk.Box):
    __gtype_name__ = 'playqueue_item'

    _title_label = GtkTemplate.Child()

    _cover_image = GtkTemplate.Child()

    _download_key = None
    _download_thumb = None

    def __init__(self, plex, item, index, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex
        self._item = item
        self._index = index

        self._connect_id = self._plex.connect("download-cover", self.__on_cover_downloaded)

        self._title_label.set_text(self._item.title)

        if (not item.TYPE == 'playlist'):
            self._download_key = item.ratingKey
            self._download_thumb = item.thumb
        elif (item.type == 'playlist'):
            self._download_key = item.ratingKey
            self._download_thumb = item.composite

        thread = threading.Thread(target=self._plex.download_cover, args=(self._download_key, self._download_thumb))
        thread.daemon = True
        thread.start()

    def get_index(self):
        return self._index

    def __on_cover_downloaded(self, plex, rating_key, path):
        if(self._download_key == rating_key):
            self._plex.disconnect(self._connect_id)
            pix = GdkPixbuf.Pixbuf.new_from_file_at_size(path, 50, 50)
            GLib.idle_add(self.__set_image, pix)

    def __set_image(self, pix):
        self._cover_image.set_from_pixbuf(pix)
