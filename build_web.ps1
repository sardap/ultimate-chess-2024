# cargo build --release --no-default-features --target wasm32-unknown-unknown
# wasm-bindgen --typescript --out-dir ./out --target web .\target\wasm32-unknown-unknown\release\uc2024.wasm
# wasm-pack build --out-dir ./out --target web
# $env:RUSTFLAGS="-C target-feature=+atomics,+bulk-memory"
wasm-pack build --out-dir ./out/pkg --target no-modules --features web

# Delete the existing uc2024 folder from the resume-site public directory
$targetFolder = "..\resume-site\public\uc2024"
if (Test-Path $targetFolder) {
    Remove-Item $targetFolder -Recurse -Force
}

# Copy the generated ./out directory to the resume-site public directory under uc2024
Copy-Item -Path "./out" -Destination $targetFolder -Recurse -Force

Write-Output "Deployment completed successfully."
