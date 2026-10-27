{ pkgs, erm }:

let
  runner = pkgs.writeShellApplication {
    name = "erm-integration-runner";
    runtimeInputs = [ pkgs.python3 ];
    text = ''
      exec python3 ${./request_matrix.py} "$@"
    '';
  };

  mkErmService =
    {
      name,
      port,
      destination,
    }:
    {
      description = "ERM integration test proxy (${name})";
      wantedBy = [ "multi-user.target" ];
      wants = [ "network-online.target" ];
      after = [ "network-online.target" ];
      environment.SSL_CERT_FILE = "/etc/ssl/certs/ca-certificates.crt";
      serviceConfig = {
        DynamicUser = true;
        StateDirectory = name;
        ExecStart = "${erm}/bin/erm --destination ${destination} --mock-dir /var/lib/${name}/mocks -p ${toString port}";
        Restart = "on-failure";
      };
    };
in
pkgs.testers.runNixOSTest {
  name = "erm-integration";
  globalTimeout = 30 * 60;
  requiredFeatures.kvm = false;

  nodes = {
    upstream =
      { lib, pkgs, ... }:
      {
        virtualisation.memorySize = 512;
        networking.firewall.allowedTCPPorts = [
          8080
          8443
        ];

        services.go-httpbin = {
          enable = true;
          settings = {
            HOST = "0.0.0.0";
            PORT = 8080;
          };
        };

        systemd.services.go-httpbin-tls = {
          description = "TLS go-httpbin for ERM integration tests";
          wantedBy = [ "multi-user.target" ];
          serviceConfig = {
            DynamicUser = true;
            ExecStart = ''
              ${lib.getExe pkgs.go-httpbin} \
                -host 0.0.0.0 \
                -port 8443 \
                -https-cert-file ${./certs/server.pem} \
                -https-key-file ${./certs/server-key.pem}
            '';
            Restart = "on-failure";
          };
        };
      };

    proxy = {
      virtualisation.memorySize = 512;
      security.pki.certificateFiles = [ ./certs/ca.pem ];
      environment.systemPackages = [ runner ];

      systemd.services = {
        erm-http = mkErmService {
          name = "erm-http";
          port = 4321;
          destination = "http://upstream:8080/anything";
        };
        erm-https = mkErmService {
          name = "erm-https";
          port = 4322;
          destination = "https://upstream:8443/anything";
        };
        erm-response = mkErmService {
          name = "erm-response";
          port = 4323;
          destination = "http://upstream:8080";
        };
      };
    };
  };

  testScript = ''
    start_all()

    upstream.wait_for_unit("go-httpbin.service")
    upstream.wait_for_unit("go-httpbin-tls.service")
    upstream.wait_for_open_port(8080)
    upstream.wait_for_open_port(8443)

    proxy.wait_for_unit("erm-http.service")
    proxy.wait_for_unit("erm-https.service")
    proxy.wait_for_unit("erm-response.service")
    proxy.wait_for_open_port(4321)
    proxy.wait_for_open_port(4322)
    proxy.wait_for_open_port(4323)

    proxy.succeed("erm-integration-runner --seed 4543053 --cases 100")

    for directory in [
        "/var/lib/erm-http/mocks",
        "/var/lib/erm-https/mocks",
        "/var/lib/erm-response/mocks",
    ]:
        proxy.wait_until_succeeds(
            f"test $(find {directory} -name 'request_*.ron' | wc -l) -gt 0"
        )
        proxy.wait_until_succeeds(
            f"test $(find {directory} -name 'response_*.ron' | wc -l) -gt 0"
        )
        proxy.succeed(
            f"cd {directory} && "
            "for request in request_*.ron; do "
            "test -f response_''${request#request_}; "
            "done && "
            "for response in response_*.ron; do "
            "test -f request_''${response#response_}; "
            "done"
        )

    # Sanitization applies only to network hops; recordings retain the decoded
    # headers that arrived from the client and upstream.
    proxy.succeed(
        "grep -q x-remove-me /var/lib/erm-http/mocks/request_*.ron"
    )
    proxy.succeed(
        "grep -q x-remove-me /var/lib/erm-https/mocks/request_*.ron"
    )
    proxy.succeed(
        "grep -q x-remove-response /var/lib/erm-response/mocks/response_*.ron"
    )
  '';
}
