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


import cairo
import threading

@Gtk.Template(resource_path='/nl/g4d/Girens/download_row.ui')
class DownloadRow(Gtk.Box):
    __gtype_name__ = 'download_row'

    _title_label = Gtk.Template.Child()
    _download_spinner = Gtk.Template.Child()

    _cover_image = Gtk.Template.Child()

    _download_key = None
    _download_thumb = None

    def __init__(self, plex, item, **kwargs):
        super().__init__(**kwargs)
        

        self._plex = plex
        self._item = item

        self._plex.connect("item-downloading", self.__on_item_downloading)
        self._plex.connect("download-cover", self.__on_cover_downloaded)

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

    def __on_cover_downloaded(self, plex, rating_key, path):
        if(self._download_key == rating_key):
            pix = GdkPixbuf.Pixbuf.new_from_file_at_size(path, 50, 50)
            GLib.idle_add(self.__set_image, pix)

    def __set_image(self, pix):
        self._cover_image.set_from_pixbuf(pix)

    def __on_item_downloading(self, plex, item, status):
        if (status == False and self._item.key == item.key):
            self._download_spinner.stop()
