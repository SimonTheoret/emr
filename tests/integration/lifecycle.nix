{ pkgs, erm }:

pkgs.testers.runNixOSTest {
  name = "erm-lifecycle";
  globalTimeout = 10 * 60;
  requiredFeatures.kvm = false;

  nodes.machine = {
    virtualisation.memorySize = 512;
    environment.systemPackages = [ pkgs.python3 ];
    users.users.erm-test = {
      isNormalUser = true;
      createHome = true;
    };
  };

  testScript = ''
    start_all()
    machine.wait_for_unit("multi-user.target")
    machine.succeed(
        "runuser -u erm-test -- python3 ${./lifecycle.py} --erm ${pkgs.lib.getExe erm}",
        timeout=300,
    )
  '';
}
