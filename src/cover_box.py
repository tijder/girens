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

from gi.repository import Gtk, GLib, GObject
from .gi_composites import GtkTemplate

import threading

@GtkTemplate(ui='/org/gnome/Plex/cover_box.ui')
class CoverBox(Gtk.Box):
    __gtype_name__ = 'cover_box'

    __gsignals__ = {
        'view-show-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,))
    }

    _title_label = GtkTemplate.Child()
    _subtitle_label = GtkTemplate.Child()
    _progress_bar = GtkTemplate.Child()
    _watched_image = GtkTemplate.Child()
    _cover_image = GtkTemplate.Child()
    _play_button = GtkTemplate.Child()

    _menu_button = GtkTemplate.Child()
    _show_view_button = GtkTemplate.Child()
    _mark_played_button = GtkTemplate.Child()
    _mark_unplayed_button = GtkTemplate.Child()

    def __init__(self, plex, item, show_view=False, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._item = item
        self._plex = plex
        self._show_view = show_view
        self._plex.connect("download-cover", self.__on_cover_downloaded)
        self._plex.connect("item-retrieved", self.__on_item_retrieved)
        self._play_button.connect("clicked", self.__on_play_button_clicked)

        self._show_view_button.connect("clicked", self.__on_go_to_show_clicked)
        self._mark_played_button.connect("clicked", self.__on_mark_played_clicked)
        self._mark_unplayed_button.connect("clicked", self.__on_mark_unplayed_clicked)

        self.__set_item(self._item)

        if (not item.TYPE == 'episode' or self._show_view):
            self._download_key = item.ratingKey
            self._download_thumb = item.thumb
        else:
            self._download_key = item.grandparentRatingKey
            self._download_thumb = item.grandparentThumb

        thread = threading.Thread(target=self._plex.download_cover, args=(self._download_key, self._download_thumb))
        thread.daemon = True
        thread.start()

    def __set_item(self, item):
        if (item.TYPE == 'movie' or item.TYPE == 'episode' and item.viewOffset != 0):
            self._progress_bar.set_fraction(item.viewOffset / item.duration)
            self._progress_bar.set_visible(True)
        else:
            self._progress_bar.set_visible(False)

        if (item.TYPE == 'episode'):
            title = item.grandparentTitle
            subtitle = item.seasonEpisode + ' - ' + item.title
            if (self._show_view):
                self._show_view_button.set_visible(False)
            else:
                self._show_view_button.set_visible(True)
        elif (item.TYPE == 'movie'):
            title = item.title
            subtitle = str(item.year)
            self._show_view_button.set_visible(False)
        elif (item.TYPE == 'season'):
            title = item.parentTitle
            subtitle = item.title
            self._show_view_button.set_visible(True)

        if (not item.isWatched and (item.TYPE == 'episode' and item.viewOffset != 0)):
            self._watched_image.set_visible(True)
            self._mark_unplayed_button.set_visible(True)
            self._mark_played_button.set_visible(True)
        elif (not item.isWatched):
            self._watched_image.set_visible(True)
            self._mark_unplayed_button.set_visible(False)
            self._mark_played_button.set_visible(True)
        else:
            self._watched_image.set_visible(False)
            self._mark_played_button.set_visible(False)
            self._mark_unplayed_button.set_visible(True)

        self._title_label.set_text(title)
        self._subtitle_label.set_text(subtitle)

    def __on_cover_downloaded(self, plex, rating_key, path):
        if(self._download_key == rating_key):
            GLib.idle_add(self.__set_image, path)

    def __set_image(self, path):
        self._cover_image.set_from_file(path)

    def __on_go_to_show_clicked(self, button):
        if self._item.TYPE == 'episode':
            self.emit('view-show-wanted', self._item.grandparentRatingKey)
        elif self._item.TYPE == 'season':
            self.emit('view-show-wanted', self._item.parentRatingKey)

    def __on_play_button_clicked(self, button):
        thread = threading.Thread(target=self._plex.play_item, args=(self._item,))
        thread.daemon = True
        thread.start()

    def __on_mark_played_clicked(self, button):
        self._menu_button.set_active(False)
        thread = threading.Thread(target=self._plex.mark_as_played, args=(self._item))
        thread.daemon = True
        thread.start()

    def __on_mark_unplayed_clicked(self, button):
        self._menu_button.set_active(False)
        thread = threading.Thread(target=self._plex.mark_as_unplayed, args=(self._item))
        thread.daemon = True
        thread.start()

    def __on_item_retrieved(self, plex, item):
        if (self._item.ratingKey == item.ratingKey):
            self._item = item
            self.__set_item(self._item)
            