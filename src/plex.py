import time
import os.path

from plexapi.myplex import MyPlexAccount
from gi.repository import GObject

class Plex(GObject.Object):
    __gsignals__ = {
        'login-status': (GObject.SIGNAL_RUN_FIRST, None, (bool,str))
    }

    def __init__(self, config_dir, data_dir, **kwargs):
        super().__init__(**kwargs)
        self._config_dir = config_dir
        self._data_dir = data_dir
        self._token = None
        if(os.path.isfile(self._config_dir + '/config')):
           with open(self._config_dir + '/config', 'r') as file:
               self._token = file.readlines()[0]

    def has_token(self):
        return self._token is not None

    def login_token(self, token):
        try:
            self._account = MyPlexAccount(token=token)
            self.__save_login(self._account._token)
            self.emit('login-status',True,'')
        except:
            self.emit('login-status',False,'Login failed')

    def login(self, username, password):
        try:
            self._account = MyPlexAccount(username, password)
            self.__save_login(self._account._token)
            self.emit('login-status',True,'')
        except:
            self.emit('login-status',False,'Login failed')

    def __save_login(self, token):
        with open(self._config_dir + '/config', 'w') as file:
                file.write(token)
            
