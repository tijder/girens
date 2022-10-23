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

import threading

@Gtk.Template(resource_path='/nl/g4d/Girens/album_item.ui')
class AlbumItem(Gtk.Box):
    __gtype_name__ = 'album_item'

    _title_label = Gtk.Template.Child()
    _time_label = Gtk.Template.Child()
    _index_label = Gtk.Template.Child()
    _play_button = Gtk.Template.Child()
    _music_clip_button = Gtk.Template.Child()

    def __init__(self, plex, item, **kwargs):
        super().__init__(**kwargs)

        self._play_button.connect("clicked", self.__on_play_button_clicked)

        self._plex = plex
        self._item = item
        if item.viewCount == 0:
            style = self._index_label.get_style_context()
            style.add_class("unplayed")

        if item.primaryExtraKey != None:
            self._music_clip_button.set_visible(True)
            self._music_clip_button.connect("clicked", self.__on_music_clip_button_clicked)
        else:
            self._music_clip_button.set_visible(False)

        self._title_label.set_text(self._item.title)
        if self._item.index is not None:
            self._index_label.set_text(self._item.index)
        con_sec, con_min, con_hour = self.__convertMillis(int(self._item.duration))

        time = ""
        if con_hour >= 1:
            time = str("{0}:{1}:{2}".format(int(con_hour), int(con_min), format(int(con_sec), '02')))
        else:
            time = str("{0}:{1}".format(int(con_min), format(int(con_sec), '02')))

        self._time_label.set_text(time)

    def __convertMillis(self, millis):
        seconds=(millis/1000)%60
        minutes=(millis/(1000*60))%60
        hours=(millis/(1000*60*60))%24
        return seconds, minutes, hours

    def __on_play_button_clicked(self, button):
        thread = threading.Thread(target=self._plex.play_item, args=(self._item,))
        thread.daemon = True
        thread.start()

    def __on_music_clip_button_clicked(self, button):
        thread = threading.Thread(target=self._plex.play_item, args=(self._item.primaryExtraKey,))
        thread.daemon = True
        thread.start()
