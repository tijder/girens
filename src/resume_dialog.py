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
from .sync_item import SyncItem

import cairo
import threading
import sys
import gc

@GtkTemplate(ui='/nl/g4d/Girens/resume_dialog.ui')
class ResumeDialog(Gtk.Dialog):
    __gtype_name__ = 'resume_dialog'

    __gsignals__ = {
        'beginning-selected': (GObject.SignalFlags.RUN_FIRST, None, (bool,)),
        'resume-selected': (GObject.SignalFlags.RUN_FIRST, None, (bool,))
    }

    _resume_button = GtkTemplate.Child()
    _beginning_button = GtkTemplate.Child()

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._resume_button.connect("clicked", self.__on_resume_clicked)
        self._beginning_button.connect("clicked", self.__on_beginning_clicked)

    def set_item(self, item):
        self.set_title("Girens ~ '" + item.title + "'")

    def __on_beginning_clicked(self, button):
        self.emit('beginning-selected',True)
        self.hide()

    def __on_resume_clicked(self, button):
        self.emit('resume-selected',True)
        self.hide()
