# Sourced: prints the --user value that owns bind-mounted files inside a container.
# Rootless Docker maps container root to the invoking user; any other uid would not.
docker_user() {
    if docker info --format '{{.SecurityOptions}}' | grep -q rootless; then
        echo "0:0"
    else
        echo "$(id -u):$(id -g)"
    fi
}
