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

from gi.repository import Gtk, GLib, GObject, GdkPixbuf

import threading

@Gtk.Template(resource_path='/nl/g4d/Girens/show_view_item.ui')
class ShowViewItem(Gtk.Box):
    __gtype_name__ = 'show_view_item'

    _cover_image = Gtk.Template.Child()
    _title_label = Gtk.Template.Child()
    _release_label = Gtk.Template.Child()
    _description_label = Gtk.Template.Child()

    _progress_bar = Gtk.Template.Child()
    _watched_image = Gtk.Template.Child()

    def __init__(self, **kwargs):
        super().__init__(**kwargs)

    def set_plex(self, plex):
        self._plex = plex
        self._plex.connect("download-cover", self.__on_cover_downloaded)

    def set_item(self, item):
        self._item = item

        release_datum = ''
        if hasattr(self._item, 'originallyAvailableAt') and self._item.originallyAvailableAt != None:
            release_datum = str(self._item.originallyAvailableAt.strftime('%d %b %Y'))

        self._title_label.set_text(str(self._item.index) + ' - ' + self._item.title)
        self._release_label.set_text(release_datum)
        self._description_label.set_text(self._item.summary)

        if item.viewOffset != 0:
            self._progress_bar.set_fraction(item.viewOffset / item.duration)
            self._progress_bar.set_visible(True)
        else:
            self._progress_bar.set_visible(False)

        if (not item.isWatched):
            self._watched_image.set_visible(True)
        else:
            self._watched_image.set_visible(False)

        self._download_key = item.ratingKey
        self._download_thumb = item.thumb

        thread = threading.Thread(target=self._plex.download_cover, args=(self._download_key, self._download_thumb))
        thread.daemon = True
        thread.start()

    def play_item(self):
        thread = threading.Thread(target=self._plex.play_item, args=(self._item,))
        thread.daemon = True
        thread.start()

    def __on_cover_downloaded(self, plex, rating_key, path):
        if(self._download_key == rating_key):
            GLib.idle_add(self.__set_image, path)
            #pix = GdkPixbuf.Pixbuf.new_from_file_at_size(path, 150, -1)
            #print(pix)
            #GLib.idle_add(self.__set_image, pix)

    def __set_image(self, pix):
        self._cover_image.set_filename(pix)
        #self._cover_image.set_from_pixbuf(pix)
