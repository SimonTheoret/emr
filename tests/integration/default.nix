{ pkgs, erm }:

let
  lib = pkgs.lib;
  upstreamHttpPort = 8080;
  upstreamHttpsPort = 8443;
  ermHttpPort = 4321;
  ermHttpsPort = 4322;
  pythonEnv = pkgs.python3.withPackages (ps: [ ps.requests ]);

  mkErmService =
    {
      name,
      port,
      destination,
    }:
    let
      mockDirectory = "/var/lib/${name}/mocks";
    in
    {
      description = "ERM integration test proxy (${name})";
      wantedBy = [ "multi-user.target" ];
      wants = [ "network-online.target" ];
      after = [ "network-online.target" ];
      environment.SSL_CERT_FILE = "/etc/ssl/certs/ca-certificates.crt";
      serviceConfig = {
        DynamicUser = true;
        StateDirectory = name;
        ExecStart = "${lib.getExe erm} --destination ${destination} --mock-dir ${mockDirectory} -p ${toString port}";
      };
    };
in
pkgs.testers.runNixOSTest {
  name = "erm-integration";

  # The NixOS test driver's globalTimeout option is expressed in seconds.
  # Without KVM (e.g. aarch64 CI runners), booting the VMs alone takes ~3 minutes.
  globalTimeout = 10 * 60;

  # Allow CI builders without /dev/kvm to run the VMs with QEMU emulation.
  requiredFeatures.kvm = false;

  nodes = {
    upstream =
      { lib, pkgs, ... }:
      {
        virtualisation.memorySize = 512;
        networking.firewall.allowedTCPPorts = [
          upstreamHttpPort
          upstreamHttpsPort
        ];

        services.go-httpbin = {
          enable = true;
          settings = {
            HOST = "0.0.0.0";
            PORT = upstreamHttpPort;
            # The 1s/5s defaults trip under QEMU emulation with parallel requests.
            SRV_READ_HEADER_TIMEOUT = "60s";
            SRV_READ_TIMEOUT = "60s";
          };
        };

        systemd.services.go-httpbin-tls = {
          description = "TLS go-httpbin for ERM integration tests";
          wantedBy = [ "multi-user.target" ];
          serviceConfig = {
            User = "go-httpbin";
            Group = "go-httpbin";
            DynamicUser = true;

            ExecStart = ''
              ${lib.getExe pkgs.go-httpbin} \
                -host 0.0.0.0 \
                -port ${toString upstreamHttpsPort} \
                -srv-read-header-timeout 60s \
                -srv-read-timeout 60s \
                -https-cert-file ${./certs/server.pem} \
                -https-key-file ${./certs/server-key.pem}
            '';

            # Match the hardening applied by the upstream NixOS module.
            AmbientCapabilities = "";
            CapabilityBoundingSet = [ "" ];
            DevicePolicy = "closed";
            LockPersonality = true;
            MemoryDenyWriteExecute = true;
            NoNewPrivileges = true;
            PrivateDevices = true;
            PrivateTmp = true;
            PrivateUsers = true;
            ProcSubset = "pid";
            ProtectClock = true;
            ProtectControlGroups = true;
            ProtectHome = true;
            ProtectHostname = true;
            ProtectKernelLogs = true;
            ProtectKernelModules = true;
            ProtectKernelTunables = true;
            ProtectProc = "invisible";
            ProtectSystem = "strict";
            RemoveIPC = true;
            RestrictAddressFamilies = [ "AF_INET AF_INET6" ];
            RestrictNamespaces = true;
            RestrictRealtime = true;
            RestrictSUIDSGID = true;
            SocketBindAllow = "tcp:${toString upstreamHttpsPort}";
            SocketBindDeny = "any";
            SystemCallArchitectures = "native";
            SystemCallFilter = [
              "@system-service"
              "~@privileged"
              "~@resources"
            ];
            UMask = "0077";
          };
        };
      };

    proxy = {
      virtualisation.memorySize = 512;
      security.pki.certificateFiles = [ ./certs/ca.pem ];
      environment.systemPackages = [ pythonEnv ];

      systemd.services = {
        erm-http = mkErmService {
          name = "erm-http";
          port = ermHttpPort;
          destination = "http://upstream:${toString upstreamHttpPort}";
        };
        erm-https = mkErmService {
          name = "erm-https";
          port = ermHttpsPort;
          destination = "https://upstream:${toString upstreamHttpsPort}";
        };
      };
    };
  };

  testScript = ''
    start_all()

    upstream.wait_for_unit("go-httpbin.service")
    upstream.wait_for_unit("go-httpbin-tls.service")
    upstream.wait_for_open_port(${toString upstreamHttpPort})
    upstream.wait_for_open_port(${toString upstreamHttpsPort})

    proxy.wait_for_unit("erm-http.service")
    proxy.wait_for_unit("erm-https.service")
    proxy.wait_for_open_port(${toString ermHttpPort})
    proxy.wait_for_open_port(${toString ermHttpsPort})

    # Random requests through both proxies, compared against go-httpbin's echo.
    proxy.succeed(
        "python3 ${./echo.py} "
        "--pair http://127.0.0.1:${toString ermHttpPort} http://upstream:${toString upstreamHttpPort} "
        "--pair http://127.0.0.1:${toString ermHttpsPort} https://upstream:${toString upstreamHttpsPort}"
    )
  '';
}
