import logging
import ctypes
import os

DLOG_DEBUG = 3
DLOG_INFO = 4
DLOG_WARN = 5
DLOG_ERROR = 6

try:
    _libdlog = ctypes.CDLL("libdlog.so.0")
    _dlog_print = _libdlog.dlog_print
    _dlog_print.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_char_p]
    _dlog_print.restype = ctypes.c_int
except OSError:
    _libdlog = None

class TizenDlogHandler(logging.Handler):
    """
    Python logging handler that routes messages to Tizen's native dlog.
    """
    def __init__(self, tag="TIZENCLAW"):
        super().__init__()
        self.tag = tag.encode('utf-8')

    def emit(self, record):
        if not _libdlog:
            return
            
        try:
            msg = self.format(record).encode('utf-8')
            prio = DLOG_INFO
            if record.levelno >= logging.ERROR:
                prio = DLOG_ERROR
            elif record.levelno >= logging.WARNING:
                prio = DLOG_WARN
            elif record.levelno <= logging.DEBUG:
                prio = DLOG_DEBUG
                
            _dlog_print(prio, self.tag, msg)
        except Exception:
            self.handleError(record)

def setup_tizen_logging():
    logger = logging.getLogger()
    logger.setLevel(logging.DEBUG)
    
    # If we are on Tizen, add dlog
    if _libdlog:
        dlog_handler = TizenDlogHandler("TIZENCLAW")
        dlog_handler.setFormatter(logging.Formatter('%(message)s'))
        logger.addHandler(dlog_handler)
    
    # Also keep basic console
    # console_handler = logging.StreamHandler()
    # console_handler.setFormatter(logging.Formatter('%(asctime)s [%(levelname)s] %(message)s'))
    # logger.addHandler(console_handler)
