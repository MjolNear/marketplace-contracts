# Marketplace Contract

This mono repo contains the source code for the smart contracts of our Open NFT Marketplace on [NEAR](https://near.org).

## Development

1. Install `rustup` via https://rustup.rs/
2. Run the following:

```
rustup default stable
rustup target add wasm32-unknown-unknown
```

### Testing

Contracts have unit tests and also integration tests using NEAR Simulation framework. All together can be run:

```
cargo test --all
```

### Compiling

You can build release version by running script:

```
./build.sh
```

### Deploying to Testnet

To deploy to Testnet, you can use next command:
```
near dev-deploy
```

This will output on the contract ID it deployed.

### Deploying to Mainnet

To deploy to Mainnet, you can use next command:
```
export NEAR_ENV=mainnet
near deploy market.mjol.near --accountId market.mjol.near
```