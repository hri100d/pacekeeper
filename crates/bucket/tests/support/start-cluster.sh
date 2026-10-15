set -eu

if [ "$#" -ne 3 ]; then
    echo "Expected three Redis ports" >&2
    exit 1
fi

addresses=""
bus_port=10000

for port in "$@"; do
    directory="/tmp/redis-$port"
    mkdir -p "$directory"

    redis-server \
        --port "$port" \
        --bind 0.0.0.0 \
        --protected-mode no \
        --save "" \
        --appendonly no \
        --cluster-enabled yes \
        --cluster-config-file nodes.conf \
        --cluster-node-timeout 5000 \
        --cluster-port "$bus_port" \
        --cluster-announce-ip 127.0.0.1 \
        --cluster-announce-port "$port" \
        --cluster-announce-bus-port "$bus_port" \
        --dir "$directory" \
        --pidfile "$directory/redis.pid" \
        --logfile "$directory/redis.log" \
        --daemonize yes

    addresses="$addresses 127.0.0.1:$port"
    bus_port=$((bus_port + 1))
done

for port in "$@"; do
    attempts=0

    until [ "$(redis-cli -p "$port" PING 2>/dev/null || true)" = "PONG" ]; do
        attempts=$((attempts + 1))

        if [ "$attempts" -ge 100 ]; then
            cat "/tmp/redis-$port/redis.log" >&2
            exit 1
        fi

        sleep 0.1
    done
done

redis-cli --cluster create $addresses \
    --cluster-replicas 0 \
    --cluster-yes

attempts=0

while :; do
    ready=true

    for port in "$@"; do
        info="$(redis-cli -p "$port" CLUSTER INFO 2>/dev/null || true)"

        if ! printf '%s\n' "$info" | grep -q 'cluster_state:ok'; then
            ready=false
        fi
    done

    if [ "$ready" = true ]; then
        break
    fi

    attempts=$((attempts + 1))

    if [ "$attempts" -ge 300 ]; then
        for port in "$@"; do
            cat "/tmp/redis-$port/redis.log" >&2
        done
        exit 1
    fi

    sleep 0.1
done

echo "PACEKEEPER_CLUSTER_READY"

while :; do
    sleep 3600
done