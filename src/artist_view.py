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

from gi.repository import Gtk, GLib, GObject
from .gi_composites import GtkTemplate
from .album_view import AlbumView

import threading

@GtkTemplate(ui='/nl/g4d/Girens/artist_view.ui')
class ArtistView(Gtk.Box):
    __gtype_name__ = 'artist_view'

    _title_label = GtkTemplate.Child()
    _subtitle_label = GtkTemplate.Child()
    _album_box = GtkTemplate.Child()

    _play_button = GtkTemplate.Child()
    _shuffle_button = GtkTemplate.Child()
    _show_more_button = GtkTemplate.Child()

    _timout = None
    _add_items_to_view = 0

    def __init__(self, plex, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex

        self._plex.connect("artist-retrieved", self.__artist_retrieved)

        self._play_button.connect("clicked", self.__on_play_button_clicked)
        self._shuffle_button.connect("clicked", self.__on_shuffle_button_clicked)
        self._show_more_button.connect("clicked", self.__show_more_clicked)

    def change_artist(self, key):
        self._title_label.set_text('')
        self._subtitle_label.set_text('')
        for item in self._album_box.get_children():
            self._album_box.remove(item)

        thread = threading.Thread(target=self._plex.get_artist, args=(key,))
        thread.daemon = True
        thread.start()

    def __artist_retrieved(self, plex, artist, albums):
        GLib.idle_add(self.__artist_process, artist, albums)

    def __artist_process(self, artist, albums):
        self._artist = artist
        self._title_label.set_text(artist.title)
        genres = ''
        for genre in artist.genres:
            genres = genres + genre.tag + " "
        self._subtitle_label.set_text(genres)

        self._albums = albums
        self._add_items_to_view = 4
        self._show_more_button.set_visible(True)
        self.__start_add_items_timout()

    def __show_more_items(self):
        self.__stop_add_items_timout()
        if len(self._albums) > 0:
            album_view = AlbumView(self._plex, artist_view=True)
            album_view.width_changed(self._screen_width)
            album_view.change_album(self._albums[0].ratingKey)
            self._album_box.add(album_view)
            self._albums.remove(self._albums[0])

            self._add_items_to_view -= 1
            if self._add_items_to_view > 0:
                album_view.connect('done-loading', self.__album_loaded)
        if len(self._albums) == 0:
            self._show_more_button.set_visible(False)

    def __stop_add_items_timout(self):
        if self._timout != None:
            GLib.source_remove(self._timout)
            self._timout = None

    def __start_add_items_timout(self):
        if len(self._albums) > 0:
            self._timout = GLib.timeout_add(50, self.__show_more_items)

    def __show_more_clicked(self, button):
        self._add_items_to_view = 8
        self.__start_add_items_timout()

    def __album_loaded(self, album):
        self.__show_more_items()

    def __on_play_button_clicked(self, button):
        thread = threading.Thread(target=self._plex.play_item, args=(self._artist,))
        thread.daemon = True
        thread.start()

    def __on_shuffle_button_clicked(self, button):
        thread = threading.Thread(target=self._plex.play_item, args=(self._artist,),kwargs={'shuffle':1})
        thread.daemon = True
        thread.start()

    def width_changed(self, width):
        self._screen_width = width
        for item in self._album_box.get_children():
            item.width_changed(width)
