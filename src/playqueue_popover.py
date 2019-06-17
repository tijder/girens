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

from .download_row import DownloadRow
from .playqueue_item import PlayqueueItem

import cairo
import threading

@GtkTemplate(ui='/nl/g4d/Girens/playqueue_popover.ui')
class PlayqueuePopover(Gtk.Popover):
    __gtype_name__ = 'playqueue_popover'

    __gsignals__ = {
        'show-button': (GObject.SignalFlags.RUN_FIRST, None, ()),
    }

    _playqueue_list = GtkTemplate.Child()

    __offset = None

    def __init__(self, plex, player, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex
        self._player = player

        self._player.connect("media-playing", self.__on_media_playing)
        self._playqueue_list.connect("row-selected", self.__on_row_selected)

    def __on_media_playing(self, player, playing, item, playqueue, offset):
        GLib.idle_add(self.__on_media_playing_process, player, playing, item, playqueue, offset)

    def __on_media_playing_process(self, player, playing, item, playqueue, offset):
        for row in self._playqueue_list.get_children():
            row.destroy()

        index = 0
        for item_queue in playqueue.items:
            playqueue_item = PlayqueueItem(self._plex, item_queue, index)
            row = self._playqueue_list.add(playqueue_item)
            if (item is not None and item.ratingKey == item_queue.ratingKey):
                self.__offset = index
                self._playqueue_list.select_row(playqueue_item.get_parent())
            index = 1 + index

    def __on_row_selected(self, listbox, listboxrow):
        if(listboxrow != None):
            child = listboxrow.get_child()
            index = child.get_index()
            if (self.__offset != index):
                self.emit("show-button")
                thread = threading.Thread(target=self._player.play_index, args=(index,))
                thread.daemon = True
                thread.start()
