# Setup a cluster of Zcash Vote Validators

1. Open an account on Tailscale
1. Generate a *reusable* key and write down the *secret* key (not the key id!)
1. Edit `config.yml`
    - Set the tailscale key as `ts_authkey`
    - Set `uid` as the user id of the current user (obtained by running `id`)
    - Add nodes
        - Each node must have a unique name
        - The HTTP election port may optionally be exported
1. Pull the docker image
    `docker pull hhanh00:zcash-vote-docker:1.2.1`
1. Run `cargo run`

A directory per node is created. Inside each directory, the `run.sh` starts a
container for the node. They can be moved to different machines or run on the
same one.

