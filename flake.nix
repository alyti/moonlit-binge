{
  inputs = {
    nixpkgs.url = "github:cachix/devenv-nixpkgs/rolling";
    systems.url = "github:nix-systems/default";
    devenv = {
      url = "github:cachix/devenv";
      inputs.nixpkgs.follows = "nixpkgs";
      # inputs.cachix.inputs.devenv.follows = "devenv";
    };

    fenix = {
      url = "github:nix-community/fenix/monthly";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  nixConfig = {
    extra-trusted-public-keys = "devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw=";
    extra-substituters = "https://devenv.cachix.org";
  };

  outputs =
    { self
    , nixpkgs
    , devenv
    , systems
    , fenix
    , ...
    }@inputs:
    let
      pkg_name = "moonlit_binge";
      forEachSystem = nixpkgs.lib.genAttrs (import systems);
    in
    {
      packages = forEachSystem (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ fenix.overlays.default ];
          };
          toolchain = pkgs.fenix.complete;
          rustPlatform = pkgs.makeRustPlatform { inherit (toolchain) cargo rustc; };
          cargoToml =
            with builtins;
            let
              toml = readFile ./Cargo.toml;
            in
            fromTOML toml;

          buildMetadata =
            with pkgs.lib.strings;
            let
              lastModifiedDate = self.lastModifiedDate or self.lastModified or "";
              date = builtins.substring 0 8 lastModifiedDate;
              shortRev = self.shortRev or "dirty";
              hasDateRev = lastModifiedDate != "" && shortRev != "";
              dot = optionalString hasDateRev ".";
            in
            "${date}${dot}${shortRev}";

          version =
            with pkgs.lib.strings;
            let
              hasBuildMetadata = buildMetadata != "";
              plus = optionalString hasBuildMetadata "+";
            in
            "${cargoToml.package.version}${plus}${buildMetadata}";

          bin = rustPlatform.buildRustPackage {
            inherit version;

            pname = pkg_name;
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            # doCheck = false; # Tests can't run inside sandbox as they need postgres and redis services...
          };

          # Converts self.lastModifiedDate to ISO-8601 for the layered image, it's cursed...
          # but at least it's pure...
          created = "${builtins.substring 0 4 self.lastModifiedDate}-${builtins.substring 4 2 self.lastModifiedDate}-${builtins.substring 6 2 self.lastModifiedDate}T${builtins.substring 8 2 self.lastModifiedDate}:${builtins.substring 10 2 self.lastModifiedDate}:${builtins.substring 12 2 self.lastModifiedDate}Z";

          dockerImage = pkgs.dockerTools.buildLayeredImage {
            inherit created;
            name = "${pkg_name}";
            tag = "v${builtins.replaceStrings [ "+" ] [ "-" ] version}";

            contents = [
              pkgs.bashInteractive
              pkgs.busybox
              pkgs.dockerTools.caCertificates
              bin
            ];

            extraCommands = ''
              set -e
              mkdir tmp
              mkdir data
              mkdir -p app/assets
              cd app
              cp -r "${./.}"/assets .
            '';

            fakeRootCommands = ''
              set -e
              ${pkgs.dockerTools.shadowSetup}
              chmod 0777 tmp
              groupadd --system -g 999 docker
              useradd --system --no-create-home -u 999 -g 999 docker
              chown -R docker:docker app data
            '';
            enableFakechroot = true;

            config = {
              Entrypoint = [ "${bin}/bin/${pkg_name}-cli" "start" ];
              Env = [ "LOCO_ENV=production" ];
              WorkingDir = "/app";
              Volumes = {
                "/app/config" = { };
                "/data" = { };
              };
              ExposedPorts = {
                "5150/tcp" = { };
              };
              User = "docker:docker";
            };
          };
        in
        {
          inherit bin dockerImage;
          devenv-up = self.devShells.${system}.default.config.procfileScript;

          default = bin;
        }
      );

      apps = forEachSystem (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          watch =
            let
              script = pkgs.writeShellScriptBin "watch" ''
                ${pkgs.systemfd}/bin/systemfd --no-pid -s http::0.0.0.0:3000 -- ${pkgs.cargo-watch}/bin/cargo-watch -w assets -w config -w migration -w src -w players -x "run -- start"
              '';
            in
            {
              type = "app";
              program = "${script}/bin/watch";
            };
        }
      );

      devShells = forEachSystem (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = devenv.lib.mkShell {
            inherit inputs pkgs;
            modules = [
              {
                # https://devenv.sh/reference/options/
                languages.rust.enable = true;
                packages = with pkgs; [
                  cargo-watch
                  cargo-insta
                  systemfd
                  jq
                  dive
                  nil
                  sea-orm-cli
                  unixtools.xxd

                  openssl
                ];

                pre-commit.hooks = {
                  statix.enable = true;
                  nixpkgs-fmt.enable = true;
                  clippy.enable = true;
                };

                services = {
                  postgres = {
                    enable = true;
                    listen_addresses = "127.0.0.1";
                    port = 5433;
                    initialDatabases = [{ name = "${pkg_name}"; }];
                    initialScript = ''
                      CREATE USER ${pkg_name} SUPERUSER PASSWORD '${pkg_name}';
                    '';
                  };

                  redis = {
                    enable = true;
                    port = 6380;
                  };
                };

                env = {
                  DATABASE_URL = "postgres://${pkg_name}:${pkg_name}@127.0.0.1:5433/${pkg_name}";
                  REDIS_URL = "redis://redis:6380";
                };
              }
            ];
          };
        }
      );
    };
}
