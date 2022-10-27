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

from gi.repository import Gtk, GLib, GObject, GdkPixbuf, Gdk, Adw

from .sync_item import SyncItem

import cairo
import threading
import sys
import gc

@Gtk.Template(resource_path='/nl/g4d/Girens/resume_dialog.ui')
class ResumeDialog(Adw.MessageDialog):
    __gtype_name__ = 'resume_dialog'

    __gsignals__ = {
        'beginning-selected': (GObject.SignalFlags.RUN_FIRST, None, (bool,)),
        'resume-selected': (GObject.SignalFlags.RUN_FIRST, None, (bool,))
    }

    def __init__(self, **kwargs):
        super().__init__(**kwargs)

    def set_item(self, item):
        self.set_heading(item.title)

    @Gtk.Template.Callback()
    def response_cb(self, widget, response):
        if(response == '_resume_button'):
            self.emit('resume-selected',True)
        elif (response == '_beginning_button'):
            self.emit('beginning-selected',True)
