use clap::{Args, Parser, Subcommand};
use cli_table::{format::Justify, print_stdout, Cell, Style, Table};
use colorize::AnsiColor;
use ethers::prelude::*;
use image::io::Reader as ImageReader;
use reqwest::Client;
use serde_json::Value;
use std::{error::Error, fs, io::Cursor, path::Path, sync::Arc};
use viuer::Config;

// rpc url sourced from ethers rs example
const IPFS_URI: &str = "https://gateway.ipfs.io/ipfs/";
const RPC_URL: &str = "https://mainnet.infura.io/v3/c60b0bb42f8a4c6481ecd229eddaca27";

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Download(Download),
    View(View),
}

#[derive(Args, Debug)]
struct Download {
    #[clap(short, long)]
    address: String,

    #[clap(short, long)]
    token_id: Option<usize>,
}

#[derive(Args, Debug)]
struct View {
    #[clap(short, long)]
    address: String,

    #[clap(short, long)]
    token_id: Option<usize>,

    #[clap(short, long)]
    show: bool,
}

abigen!(
    IERC721,
    r#"[
        function baseURI() internal view virtual returns (string memory)
        function symbol() external view returns (string memory)
        function totalSupply() public view virtual override returns (uint256)
        function name() external view returns (string memory)

        function tokenURI(uint256 tokenId) public view virtual override returns (string memory)
    ]"#,
);

struct Collection {
    uri: String,
    symbol: String,
    total_supply: U256,
    name: String,
}

impl Collection {
    async fn new(
        pair: &ierc721_mod::IERC721<ethers::providers::Provider<ethers::providers::Http>>,
    ) -> Self {
        // collection
        let uri = pair.base_uri().call().await.unwrap_or_default();
        let symbol = pair.symbol().call().await.unwrap_or_default();
        let total_supply = pair.total_supply().call().await.unwrap_or_default();
        let name = pair.name().call().await.unwrap_or_default();

        Collection {
            uri,
            symbol,
            total_supply,
            name,
        }
    }
}

struct Nft {
    token_id: String,
    image_url: String,
    token_uri: String,
    attributes: Vec<Value>,
}

impl Nft {
    async fn new(
        token_id: &U256,
        pair: &ierc721_mod::IERC721<ethers::providers::Provider<ethers::providers::Http>>,
    ) -> Result<Self, Box<dyn Error>> {
        let token_uri = pair.token_uri(*token_id).call().await.unwrap_or_default();

        let mut request_address = token_uri.clone();
        if token_uri.starts_with("ipfs://") {
            request_address = format!(
                "{}{}",
                IPFS_URI,
                token_uri.chars().skip(7).collect::<String>()
            );
        }

        let res = Client::new()
            .get(request_address.clone())
            .send()
            .await?
            .text()
            .await?;

        let v: Value = serde_json::from_str(&res)?;

        let image_url = String::from(v["image"].as_str().unwrap());

        let attributes = v["attributes"].as_array().unwrap();

        Ok(Nft {
            token_id: token_id.to_string(),
            image_url,
            token_uri: token_uri.to_string(),
            attributes: attributes.to_vec(),
        })
    }
}

struct Img {}

impl Img {
    async fn new(attributes: Vec<Value>, image_url: String) -> Result<Self, Box<dyn Error>> {
        let conf = Config {
            // set offset
            x: 1,
            y: 36 + (attributes.len() as f32 * 1.4) as i16,
            // set dimensions
            width: Some(80),
            height: Some(40),
            ..Default::default()
        };

        let mut image_request_address = image_url.clone();
        if image_url.starts_with("ipfs://") {
            image_request_address = format!(
                "{}{}",
                IPFS_URI,
                image_url.chars().skip(7).collect::<String>()
            );
        }

        let image_res = Client::new()
            .get(image_request_address.clone())
            .send()
            .await?
            .bytes()
            .await?;

        let img = ImageReader::new(Cursor::new(image_res))
            .with_guessed_format()?
            .decode()?;

        viuer::print(&img, &conf).expect("Image printing failed.");

        Ok(Img {})
    }
    async fn save(path: &Path, image_url: String) -> Result<Self, Box<dyn Error>> {
        let mut image_request_address = image_url.clone();
        if image_url.starts_with("ipfs://") {
            image_request_address = format!(
                "{}{}",
                IPFS_URI,
                image_url.chars().skip(7).collect::<String>()
            );
        }

        let image_res = Client::new()
            .get(image_request_address.clone())
            .send()
            .await?
            .bytes()
            .await?;

        let img = ImageReader::new(Cursor::new(image_res))
            .with_guessed_format()?
            .decode()?;

        img.save(path)?;

        Ok(Img {})
    }
}

struct Viewer {}

impl Viewer {
    async fn save(address: &str, nft: Option<Nft>) -> Result<(), Box<dyn Error>> {
        // clear terminal
        if let Some(nft) = nft {
            fs::create_dir_all(&format!("{}/", address))?;
            Img::save(
                Path::new(&format!("{}/{}.png", address, nft.token_id)),
                nft.image_url,
            )
            .await?;
        }

        Ok(())
    }

    async fn show(
        collection: Collection,
        nft: Option<Nft>,
        address: Address,
        show_image: bool,
    ) -> Result<(), Box<dyn Error>> {
        // clear terminal
        print!("{esc}[2J{esc}[1;1H", esc = 27 as char);

        println!("{}", "͕→ Collection".yellow().bold());

        let table = vec![
            vec![
                "Name".cell(),
                collection.name.cell().justify(Justify::Right),
            ],
            vec!["Address".cell(), address.cell().justify(Justify::Right)],
            vec!["Uri".cell(), collection.uri.cell().justify(Justify::Right)],
            vec![
                "Symbol".cell(),
                collection.symbol.cell().justify(Justify::Right),
            ],
            vec![
                "Total Supply".cell(),
                collection.total_supply.cell().justify(Justify::Right),
            ],
        ]
        .table()
        .title(vec!["Key".cell().bold(true), "Value".cell().bold(true)])
        .bold(true);

        assert!(print_stdout(table).is_ok());

        if let Some(nft) = nft {
            println!();
            println!("{}", "→ Single".green().bold());

            let table = vec![
                vec![
                    "Token ID".cell(),
                    nft.token_id.cell().justify(Justify::Right),
                ],
                vec![
                    "Token Uri".cell(),
                    nft.token_uri.cell().justify(Justify::Right),
                ],
                vec![
                    "Image Url".cell(),
                    nft.image_url.clone().cell().justify(Justify::Right),
                ],
            ]
            .table()
            .title(vec!["Key".cell().bold(true), "Value".cell().bold(true)])
            .bold(true);

            assert!(print_stdout(table).is_ok());

            println!();
            println!("{}", "→ Attributes".blue().bold());

            let mut attrs = vec![];

            for attr in &nft.attributes {
                let k = attr["trait_type"].as_str().unwrap();
                let v = attr["value"].as_str().unwrap();
                attrs.push(vec![k.cell(), v.cell().justify(Justify::Right)])
            }

            let table = attrs
                .table()
                .title(vec!["Key".cell().bold(true), "Value".cell().bold(true)])
                .bold(true);

            assert!(print_stdout(table).is_ok());

            if show_image {
                println!();
                println!("{}", "→ Image".red().bold());

                Img::new(nft.attributes, nft.image_url).await?;
            }
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let client = Provider::<Http>::try_from(RPC_URL)?;
    let client = Arc::new(client);

    match &cli.command {
        Commands::View(project) => {
            let address = project.address.parse::<Address>().unwrap();

            let pair = IERC721::new(address, Arc::clone(&client));
            let collection = Collection::new(&pair).await;

            let mut nft = None;

            if let Some(token_id) = project.token_id {
                let token_id = U256::from(token_id);
                nft = Some(Nft::new(&token_id, &pair).await?);
            };

            Viewer::show(collection, nft, address, project.show).await?;
        }
        Commands::Download(project) => {
            let address = project.address.parse::<Address>().unwrap();

            let pair = IERC721::new(address, Arc::clone(&client));
            let collection = Collection::new(&pair).await;

            // if token id, just download that one
            if let Some(token_id) = project.token_id {
                let token_id = U256::from(token_id);
                println!("Downloading:  {}", token_id.to_string().yellow());
                let nft = Some(Nft::new(&token_id, &pair).await?);
                Viewer::save(&project.address, nft).await?;
            };

            // no token. download all of them
            if project.token_id.is_none() {
                for _token_id in 0..collection.total_supply.as_u64() {
                    let token_id = U256::from(_token_id);
                    println!("Downloading:  {}", token_id.to_string().yellow());
                    let nft = Some(Nft::new(&token_id, &pair).await?);
                    Viewer::save(&project.address, nft).await?;
                }
            }
        }
    }

    Ok(())
}
