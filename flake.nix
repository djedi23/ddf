{
  description = "Build a cargo project";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.rust-analyzer-src.follows = "";
    };

    flake-utils.url = "github:numtide/flake-utils";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, crane, fenix, flake-utils, advisory-db, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        inherit (pkgs) lib;

        craneLib = crane.mkLib nixpkgs.legacyPackages.${system};
        src = craneLib.cleanCargoSource (craneLib.path ./.);

        # Common arguments can be set here to avoid repeating them later
        commonArgs = {
          inherit src;
          strictDeps = true;
          pname = "ddf";

          nativeBuildInputs = with pkgs; [ installShellFiles makeWrapper gzip ];
          buildInputs = [
            # Add additional build inputs here
          ] ++ lib.optionals pkgs.stdenv.isDarwin [
            # Additional darwin specific inputs can be set here
            pkgs.libiconv
          ];

          # Additional environment variables can be set directly
          # MY_CUSTOM_VAR = "some value";
          LD_LIBRARY_PATH = "${lib.makeLibraryPath commonArgs.buildInputs}";
        };

        craneLibLLvmTools = craneLib.overrideToolchain
          (fenix.packages.${system}.complete.withComponents [
            "cargo"
            "llvm-tools"
            "rustc"
          ]);

        # Build *just* the cargo dependencies, so we can reuse
        # all of that work (e.g. via cachix) when running in CI
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        # Build the actual crate itself, reusing the dependency
        # artifacts from above.
        ddf = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          postInstall = ''
                        installShellCompletion --cmd ${commonArgs.pname} --bash <($out/bin/${commonArgs.pname} --completion bash) --fish <($out/bin/${commonArgs.pname} --completion fish) --zsh <($out/bin/${commonArgs.pname} --completion zsh)
          '';
          #             wrapProgram "$out/bin/${pname}" --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath commonArgs.buildInputs}"
        });
      in
      {
        checks = {
          # Build the crate as part of `nix flake check` for convenience
          inherit ddf;

          # Run clippy (and deny all warnings) on the crate source,
          # again, resuing the dependency artifacts from above.
          #
          # Note that this is done as a separate derivation so that
          # we can block the CI if there are issues here, but not
          # prevent downstream consumers from building our crate by itself.
          ddf-clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

          ddf-doc = craneLib.cargoDoc (commonArgs // {
            inherit cargoArtifacts;
          });

          # Check formatting
          ddf-fmt = craneLib.cargoFmt {
            inherit src;
          };

          # Audit dependencies
          ddf-audit = craneLib.cargoAudit {
            inherit src advisory-db;
          };

          # Audit licenses
#          ddf-deny = craneLib.cargoDeny {
#            inherit src;
#          };

          # Run tests with cargo-nextest
          # Consider setting `doCheck = false` on `ddf` if you do not want
          # the tests to run twice
          ddf-nextest = craneLib.cargoNextest (commonArgs // {
            inherit cargoArtifacts;
            partitions = 1;
            partitionType = "count";
          });
        };

        packages = {
          default = ddf;
        } // lib.optionalAttrs (!pkgs.stdenv.isDarwin) {
          ddf-llvm-coverage = craneLibLLvmTools.cargoLlvmCov (commonArgs // {
            inherit cargoArtifacts;
          });
        };

        apps.default = flake-utils.lib.mkApp {
          drv = ddf;
        };

        devShells.default = craneLib.devShell {
          # Inherit inputs from checks.
          checks = self.checks.${system};

          # Additional dev-shell environment variables can be set directly
          # MY_CUSTOM_DEVELOPMENT_VAR = "something else";
          RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
          # LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath commonArgs.buildInputs}";

          # Extra inputs can be added here; cargo and rustc are provided by default.
          packages = with pkgs; [
            # pkgs.ripgrep
            bacon
          ];
        };
      });
}
