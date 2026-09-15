import type { ComponentAdapter } from "$lib/types";
import HadoopConfigFields from "./HadoopConfigFields.svelte";
import HadoopInstallFields from "./HadoopInstallFields.svelte";
import HadoopLogo from "./HadoopLogo.svelte";
import { HADOOP_CONFIG_FIELD_IDS, HADOOP_INSTALL_PARAM_IDS } from "./fields";

export const hadoopAdapter: ComponentAdapter = {
  id: "hadoop",
  order: 10,
  displayName: "Hadoop",
  logo: HadoopLogo,
  installFields: HadoopInstallFields,
  configFields: HadoopConfigFields,
  expectedInstallParamIds: () => HADOOP_INSTALL_PARAM_IDS,
  expectedConfigFieldIds: () => HADOOP_CONFIG_FIELD_IDS,
};
