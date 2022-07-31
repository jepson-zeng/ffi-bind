#!/bin/bash
exec 3<>/dev/udp/127.0.0.1/3829
echo "get match" >&3
cat <&3
