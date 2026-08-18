self: rec {
  default = my-cloud;
  my-cloud =
    {
      config,
      pkgs,
      lib,
      ...
    }:
    let
      cfg = config.services.my-cloud;
      system = pkgs.stdenv.hostPlatform.system;
      settingsFormat = pkgs.formats.toml { };
      hasFeature = feature: cfg.features == null || builtins.elem feature cfg.features;
    in
    with lib;
    with types;
    {
      options.services.my-cloud = {
        enable = mkEnableOption "my-cloud";
        package = mkOption {
          type = package;
          default = self.packages.${system}.default;
          defaultText = literalExpression "self.packages.\${system}.default";
          description = "The package to use for my-cloud.";
        };
        features = mkOption {
          type = nullOr (listOf (enum [ "webdav" ]));
          default = null;
          description = "The features to enable for my-cloud. No value means all (the default).";
        };
        finalPackage = mkOption {
          type = package;
          internal = true;
          readOnly = true;
        };
        user = mkOption {
          type = str;
          default = "my-cloud";
          description = "The user to run my-cloud as.";
        };
        group = mkOption {
          type = str;
          default = "my-cloud";
          description = "The group to run my-cloud as.";
        };
        openFirewall = mkOption {
          type = bool;
          default = false;
          description = "Whether to open the firewall for my-cloud.";
        };
        settings = mkOption {
          type = submodule {
            freeformType = settingsFormat.type;
            options = {
              webdav = {
                host = mkOption {
                  type = str;
                  default = "127.0.0.1";
                };
                port = mkOption {
                  type = port;
                  default = 5000;
                };
                root = mkOption {
                  type = str;
                  default = "/srv/my-cloud";
                };
              };
              database = {
                url = mkOption {
                  type = str;
                  default = "sqlite:///var/lib/my-cloud/database.db";
                };
              };
            };
          };
          description = "Settings for my-cloud.";
        };
      };
      config = mkIf cfg.enable {
        services.my-cloud.finalPackage =
          if cfg.features == null then
            cfg.package
          else
            cfg.package.overrideAttrs (old: {
              cargoBuildFlags =
                (old.cargoBuildFlags or [ ])
                ++ [ "--no-default-features" ]
                ++ map (x: "--features=${x}") cfg.features;
            });
        users.users = mkIf (cfg.user == "my-cloud") {
          my-cloud = {
            inherit (cfg) group;
            isSystemUser = true;
          };
        };
        users.groups = mkIf (cfg.group == "my-cloud") {
          my-cloud = { };
        };
        networking.firewall.allowedTCPPorts = mkIf (cfg.openFirewall && hasFeature "webdav") [
          cfg.settings.webdav.port
        ];
        systemd.tmpfiles.rules = optionals (hasFeature "webdav") [
          "d ${cfg.settings.webdav.root} 0770 ${cfg.user} ${cfg.group} -"
        ];
        systemd.services.my-cloud = {
          description = "My Own Cloud Server";
          after = [ "network.target" ];
          wantedBy = [ "multi-user.target" ];
          serviceConfig = {
            ExecStart = "${getExe cfg.finalPackage} --config ${settingsFormat.generate "my-cloud.toml" cfg.settings}";
            User = cfg.user;
            Group = cfg.group;
            StateDirectory = "my-cloud";
            WorkingDirectory = "/var/lib/my-cloud";
            RestrictAddressFamilies = [
              "AF_UNIX"
              "AF_INET"
              "AF_INET6"
            ];
            RestrictNamespaces = true;
            ProtectClock = true;
            ProtectControlGroups = true;
            ProtectHome = true;
            ProtectKernelLogs = true;
            ProtectKernelModules = true;
            ProtectKernelTunables = true;
          };
        };
      };
    };
}
