%global debug_package %{nil}
%global _build_id_links none

Name:           nexus-boot
Version:        %{nexus_version}
Release:        1%{?dist}
Summary:        Painel de gerenciamento da inicialização UEFI
License:        GPL-3.0-only AND Ubuntu-font-1.0
URL:            https://github.com/eritongrnascimento-svg/nexus-boot
ExclusiveArch:  x86_64

# The GUI loads these libraries dynamically on X11 and Wayland.
Requires:       libglvnd-egl, libglvnd-glx, libX11, libX11-xcb
Requires:       libXcursor, libXi, libXrandr, libxkbcommon, libxkbcommon-x11
Requires:       libwayland-client, libwayland-egl
Requires:       bash, (coreutils or coreutils-single), gawk, grep, polkit
Requires:       util-linux, util-linux-core, systemd, systemd-udev
Requires:       dosfstools, efibootmgr, e2fsprogs
Recommends:     mesa-dri-drivers

%description
Painel para inventário de discos, planejamento de partições e gerenciamento
de inicialização UEFI. Inclui o seletor EFI do Nexus Boot.

%install
install -Dm755 "%{manager_binary}" "%{buildroot}%{_bindir}/nexus-boot"
install -Dm644 "%{project_dir}/crates/nexus-manager/src/nexus-boot.png" "%{buildroot}%{_datadir}/icons/hicolor/64x64/apps/nexus-boot.png"
install -Dm644 "%{project_dir}/packaging/nexus-boot.desktop" "%{buildroot}%{_datadir}/applications/nexus-boot.desktop"
install -Dm644 "%{project_dir}/packaging/io.github.eritongrnascimento_svg.NexusBoot.metainfo.xml" "%{buildroot}%{_metainfodir}/io.github.eritongrnascimento_svg.NexusBoot.metainfo.xml"
install -Dm644 "%{project_dir}/LICENSE" "%{buildroot}%{_licensedir}/nexus-boot/LICENSE"
install -Dm644 "%{project_dir}/crates/nexus-efi/assets/UFL.txt" "%{buildroot}%{_licensedir}/nexus-boot/UFL.txt"
install -Dm644 "%{project_dir}/TRADEMARKS.md" "%{buildroot}%{_docdir}/nexus-boot/TRADEMARKS.md"
install -Dm644 "%{project_dir}/README.md" "%{buildroot}%{_docdir}/nexus-boot/README.md"

%files
%{_bindir}/nexus-boot
%{_datadir}/icons/hicolor/64x64/apps/nexus-boot.png
%{_datadir}/applications/nexus-boot.desktop
%{_metainfodir}/io.github.eritongrnascimento_svg.NexusBoot.metainfo.xml
%license %{_licensedir}/nexus-boot/
%doc %{_docdir}/nexus-boot/
