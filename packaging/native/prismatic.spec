%global debug_package %{nil}
Name: prismatic
Version: @VERSION@
Release: 1%{?dist}
Summary: Happy Years SGI workstation emulator
License: BSD-3-Clause
URL: https://github.com/salvogendut/prismatic
Source0: payload.tar
Requires: hicolor-icon-theme
Recommends: ffmpeg

%description
Prismatic is a Rust and SDL3 frontend for the Iris SGI workstation emulator.
This binary package is built with the Iris xiri core in an isolated CI checkout.

%prep
mkdir payload
tar -xf %{SOURCE0} -C payload

%install
mkdir -p %{buildroot}
cp -a payload/usr %{buildroot}/

%files
%{_bindir}/prismatic
%{_libexecdir}/prismatic/
%{_datadir}/applications/io.github.salvogendut.Prismatic.desktop
%{_datadir}/metainfo/io.github.salvogendut.Prismatic.metainfo.xml
%{_datadir}/icons/hicolor/512x512/apps/io.github.salvogendut.Prismatic.png
%license %{_datadir}/licenses/prismatic
%doc %{_datadir}/doc/prismatic
