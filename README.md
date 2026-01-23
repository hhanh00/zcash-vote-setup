# Server

Edit `server_config.yml`

Run `cargo r --bin server`

# Node

Run `cargo r --bin client <server_url> <node name`
Repeat with each node. Once every node is added
the configuration is completed.
Repeat the same command on every node once to get
the configuration.

Ex:
```
# With three nodes
client http://localhost:9000 node1
client http://localhost:9000 node2
client http://localhost:9000 node3
# node 3 has its configuration, but node1 and node2 have
# to be called again
client http://localhost:9000 node1
client http://localhost:9000 node2
```

Run `bash run.sh`

## To reset

- remove the contents of `work/home/data,db`
- delete the content of the table votedb in `setup.db`
- rerun the server
