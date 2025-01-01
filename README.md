# SimBA: Simulating BFT Applications

This project simulates blockchain protocols using the `asim` crate. It also provides a GUI that can visualize the protocols execution and mans to export and plot metrics.

## Compilation 
* Invoking `just build` will generate all SimBA binaries and `just install` will install them locally
* You might want to set the buildtype to release for better performance: `just BUILDTYPE=release install`

### Required Packages
* LLVM, Clang, and LLD
* just: `cargo install just`

## Supported Protocols
SimBA can simulate PBFT, Bitcoin, and Ethereum. There is work on the way for Avalanche and Ethereum 2.0.

## Using SimBA
### Command-Line Interface 
`simba` is the command line interface to run experiments. 

## Desktop UI
`simba-visualizer` provies a graphical userinterface using wgpu and iced to inspect the blockchain network as the simulation executes.

## Web UI
One goal of this simulator is to provide web support, so that it can easily be embedded into blog posts and online tutorials.

However, the web interface is currently broken as no easy way exist to support certain tokio features in a Web context (yet).
Help to resolve this limitation is very welcome.

## Know Problems and FAQ 
### No Text Rendered
SimBA uses system fonts. The intended font is Fira Sans, but it should fall back to other fonts if it is not available.

However, in some setups you might need to install Fira Sans or fontconfig.
