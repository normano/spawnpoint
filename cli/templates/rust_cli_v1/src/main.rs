use clap::Parser;

/// A simple CLI application: --crate-description--
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
   /// Name of the person to greet
   #[arg(short, long, default_value = "World")]
   name: String,

   /// Number of times to greet
   #[arg(short, long, default_value_t = 1)]
   count: u8,
}

// Example using a transformed variable name (though maybe less common in main)
// struct __PascalCrateName__Config {}

fn main() {
    let args = Args::parse();

    for _ in 0..args.count {
        println!("Hello {}!", args.name);
    }
    // Example usage of description placeholder
    // let _description = "--crate-description--";
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        // Basic test placeholder
        assert_eq!(2 + 2, 4);
    }
}