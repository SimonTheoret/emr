{ pkgs, erm }:

let
  lib = pkgs.lib;
  upstreamHttpPort = 8080;
  upstreamHttpsPort = 8443;
  ermHttpPort = 4321;
  ermHttpsPort = 4322;

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
  globalTimeout = 5 * 60;

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
      environment.systemPackages = [ pkgs.curl ];

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
    import json

    start_all()

    upstream.wait_for_unit("go-httpbin.service")
    upstream.wait_for_unit("go-httpbin-tls.service")
    upstream.wait_for_open_port(${toString upstreamHttpPort})
    upstream.wait_for_open_port(${toString upstreamHttpsPort})

    proxy.wait_for_unit("erm-http.service")
    proxy.wait_for_unit("erm-https.service")
    proxy.wait_for_open_port(${toString ermHttpPort})
    proxy.wait_for_open_port(${toString ermHttpsPort})

    for scheme, proxy_port, upstream_port in [
        ("http", ${toString ermHttpPort}, ${toString upstreamHttpPort}),
        ("https", ${toString ermHttpsPort}, ${toString upstreamHttpsPort}),
    ]:
        response = proxy.succeed(
            "curl --fail --silent --show-error "
            "--request POST "
            "--header 'Content-Type: text/plain' "
            "--header 'X-Test: value' "
            "--header 'X-Repeat: first' "
            "--header 'X-Repeat: second' "
            "--header 'Connection: x-remove-me' "
            "--header 'X-Remove-Me: secret' "
            "--data 'echo-body' "
            f"http://127.0.0.1:{proxy_port}/anything/echo?value=one"
        )
        document = json.loads(response)
        headers = {
            name.lower(): values for name, values in document["headers"].items()
        }

        assert document["method"] == "POST"
        assert document["url"] == (
            f"{scheme}://upstream:{upstream_port}/anything/echo?value=one"
        )
        assert document["data"] == "echo-body"
        assert headers["host"] == [f"upstream:{upstream_port}"]
        assert headers["x-test"] == ["value"]
        assert headers["x-repeat"] == ["first", "second"]
        assert "connection" not in headers
        assert "x-remove-me" not in headers
  '';
}
