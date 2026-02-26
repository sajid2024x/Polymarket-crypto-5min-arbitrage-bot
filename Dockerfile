FROM rustlang/rust:nightly

WORKDIR /app

# copy everything
COPY . .

# build the bot
RUN cargo build --release

# run the bot
CMD ["./target/release/poly_5min_bot"]
