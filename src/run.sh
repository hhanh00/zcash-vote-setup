#!/bin/bash
cp /Rocket.toml /home/user
/usr/bin/supervisord -c /etc/supervisord.conf &
tailscale login --auth-key=$TS_AUTHKEY --hostname=$NODE
tailscale up
sleep 30
supervisorctl start zcash-vote-server
supervisorctl start cometbft
wait
