import event_log

event_log.events.append("leaf")
from .. import sibling
from . import peer

VALUE = sibling.VALUE + 25
