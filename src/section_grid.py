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

@GtkTemplate(ui='/nl/g4d/Girens/section_grid.ui')
class SectionGrid(Gtk.Grid):
    __gtype_name__ = 'section_grid'

    __gsignals__ = {
        'section-clicked': (GObject.SignalFlags.RUN_FIRST, None, (object,))
    }

    _title_button = GtkTemplate.Child()
    _data = None
    _title = None

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

    def set_data(self, data):
        self._data = data

    def get_data(self):
        return self._data

    def set_title(self, title):
        self._title = title
        self._title_button.set_label(title)

    def set_custom_title(self, title):
        self._title_button.set_label(title)

    def get_title(self):
        return self._title
