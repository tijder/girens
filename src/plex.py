import time

from gi.repository import GObject

class Plex(GObject.Object):
    __gsignals__ = {
        'login-status': (GObject.SIGNAL_RUN_FIRST, None, (bool,str))
    }

    def __init__(self, **kwargs):
        super().__init__(**kwargs)

    def login(self, username, password):
        time.sleep(3)
        print(username)
        print(password)
        self.emit('login-status',False,'Error')
