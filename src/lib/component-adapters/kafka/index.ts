import type { ComponentAdapter } from "$lib/types";
import KafkaConfigFields from "./KafkaConfigFields.svelte";
import KafkaInstallFields from "./KafkaInstallFields.svelte";
import KafkaLogo from "./KafkaLogo.svelte";
import { KAFKA_CONFIG_FIELD_IDS, KAFKA_INSTALL_PARAM_IDS } from "./fields";

export const kafkaAdapter: ComponentAdapter = {
  id: "kafka",
  order: 20,
  displayName: "Kafka",
  logo: KafkaLogo,
  installFields: KafkaInstallFields,
  configFields: KafkaConfigFields,
  expectedInstallParamIds: () => KAFKA_INSTALL_PARAM_IDS,
  expectedConfigFieldIds: () => KAFKA_CONFIG_FIELD_IDS,
};
