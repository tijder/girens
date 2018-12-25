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

@GtkTemplate(ui='/nl/g4d/Girens/loading_view.ui')
class LoadingView(Gtk.Box):
    __gtype_name__ = 'loading_view'

    __gsignals__ = {
        'view-show-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,))
    }

    _loading_text_label = GtkTemplate.Child()

    def __init__(self, plex, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex

    def set_text(self, loading_text):
        self._loading_text_label.set_text(loading_text)
