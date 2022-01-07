import { getTree } from "../api_access/get_tree";
import { normalizeData } from "../data/normalize";
import { Input } from "../gui/input";
import { renderTree } from "../gui/render_tree";
import { clearChildNodes } from "../gui/util";

export const treeMain = async(GlobalConfig) => {

    const run = () => {
        clearChildNodes(GlobalConfig.outputElemId);

        getTree(
            Input.treeRootNode.parseString(),
            Input.limit.parseInt(),
            Input.depth.parseInt(),
        )
        .then((dataTree) => {
            normalizeData(
                dataTree,
                (data) => data.nodes.map((node) => node.weight),
                (data, normalizedWeights) => {
                    normalizedWeights.forEach((normalizedWeight, i) => {
                        data.nodes[i].weight = normalizedWeight;
                    });
                },
                {
                    newMin: GlobalConfig.minWeight,
                    newMax: GlobalConfig.maxWeight,
                }
            );
            renderTree(
                dataTree,
                document.getElementById(GlobalConfig.outputElemId),
            );
        });
    };
    run();

    Input.elems.forEach((elem) => {
        if (elem === null) {
            return;
        }
        elem.addEventListener("focusout", run);
    });    
};