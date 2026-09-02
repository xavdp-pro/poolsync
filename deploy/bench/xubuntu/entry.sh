#!/bin/sh
# clés ssh depuis /data/authorized_keys (même convention que les bureaux Neko)
if [ -f /data/authorized_keys ]; then
  mkdir -p /home/zaza/.ssh && cp /data/authorized_keys /home/zaza/.ssh/authorized_keys
  chown -R zaza:zaza /home/zaza/.ssh && chmod 700 /home/zaza/.ssh && chmod 600 /home/zaza/.ssh/authorized_keys
fi
exec /usr/bin/supervisord -n -c /etc/supervisor/supervisord.conf
